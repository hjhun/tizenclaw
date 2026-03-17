/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

/**
 * tizenclaw-tool-executor — Host-native C++ tool execution daemon.
 *
 * Listens on an abstract namespace Unix domain socket and executes
 * tool scripts on the host Linux directly.  Python code is run
 * in-process via linked libpython (Py_Initialize / PyRun_SimpleString).
 *
 * Protocol: 4-byte big-endian length prefix + UTF-8 JSON body
 * Security: SO_PEERCRED validates peer is tizenclaw or tizenclaw-cli.
 */

#include <Python.h>

#include <arpa/inet.h>
#include <fcntl.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include <json.hpp>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_TOOL_EXECUTOR"

#include "../common/logging.hh"

namespace {

// ─── Constants ──────────────────────────────────────────────
constexpr const char kSocketName[] = "tizenclaw-tool-executor.sock";
constexpr size_t kSocketNameLen = sizeof(kSocketName);
constexpr const char kSandboxSocketName[] = "tizenclaw-code-sandbox.sock";
constexpr size_t kSandboxSocketNameLen = sizeof(kSandboxSocketName);
constexpr size_t kMaxPayload = 10 * 1024 * 1024;
constexpr int kExecTimeout = 30;
constexpr int kCodeExecTimeout = 15;

const std::string kAppDataDir = "/opt/usr/share/tizenclaw";
const std::string kSkillsDir = kAppDataDir + "/tools/skills";
const std::string kCustomSkillsDir = kAppDataDir + "/tools/custom_skills";

const std::vector<std::string> kAllowedCallers = {
    "tizenclaw",
    "tizenclaw-cli",
};

volatile sig_atomic_t g_running = 1;
void SignalHandler(int) { g_running = 0; }

// ─── Embedded Python (direct link) ──────────────────────────
static std::mutex g_py_mutex;
static bool g_py_initialized = false;

bool InitPython() {
  std::lock_guard<std::mutex> lock(g_py_mutex);
  if (g_py_initialized) return true;

  Py_Initialize();
  if (!Py_IsInitialized()) {
    LOG(ERROR) << "Py_Initialize() failed";
    return false;
  }

  g_py_initialized = true;
  LOG(INFO) << "Python interpreter initialized (linked)";
  return true;
}

/// Run Python code in-process and capture stdout/stderr.
std::pair<std::string, int> RunPythonCode(const std::string& code) {
  std::lock_guard<std::mutex> lock(g_py_mutex);
  if (!g_py_initialized) return {"Python not initialized", -1};

  // Create temp file for captured output
  char out_path[] = "/tmp/tizenclaw_pyout_XXXXXX";
  int fd = mkstemp(out_path);
  if (fd < 0) return {"Failed to create temp file", -1};
  close(fd);

  // Wrap user code: redirect stdout/stderr to temp file
  std::string wrapper =
      "import sys as _sys, io as _io\n"
      "_orig_stdout, _orig_stderr = _sys.stdout, _sys.stderr\n"
      "_buf = _io.StringIO()\n"
      "_sys.stdout = _sys.stderr = _buf\n"
      "try:\n";

  // Indent each line of user code
  std::istringstream iss(code);
  std::string line;
  while (std::getline(iss, line)) {
    wrapper += "    " + line + "\n";
  }

  wrapper +=
      "except Exception as _e:\n"
      "    print(f'Error: {_e}', file=_buf)\n"
      "finally:\n"
      "    _sys.stdout, _sys.stderr = _orig_stdout, _orig_stderr\n"
      "    with open('" + std::string(out_path) + "', 'w') as _f:\n"
      "        _f.write(_buf.getvalue())\n";

  int rc = PyRun_SimpleString(wrapper.c_str());

  // Read captured output
  std::string output;
  std::ifstream ifs(out_path);
  if (ifs.is_open()) {
    output.assign(std::istreambuf_iterator<char>(ifs),
                  std::istreambuf_iterator<char>());
  }
  unlink(out_path);

  return {output, rc};
}

// ─── Shell helpers ──────────────────────────────────────────
std::string EscapeShellArg(const std::string& s) {
  std::string out = "'";
  for (char c : s) {
    if (c == '\'') out += "'\\''";
    else out += c;
  }
  out += "'";
  return out;
}

std::pair<std::string, int> RunCommand(const std::string& cmd,
                                        int timeout_sec = kExecTimeout) {
  int pipefd[2];
  if (pipe2(pipefd, O_CLOEXEC) == -1) return {"", -1};

  pid_t pid = fork();
  if (pid == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    return {"", -1};
  }

  if (pid == 0) {
    close(pipefd[0]);
    dup2(pipefd[1], STDOUT_FILENO);
    dup2(pipefd[1], STDERR_FILENO);
    close(pipefd[1]);
    const char* shell = "/bin/bash";
    if (access(shell, X_OK) != 0) shell = "/bin/sh";
    execl(shell, shell, "-c", cmd.c_str(), nullptr);
    _exit(127);
  }

  close(pipefd[1]);
  std::string output;
  char buf[4096];

  ssize_t n;
  while ((n = read(pipefd[0], buf, sizeof(buf))) > 0) {
    output.append(buf, n);
    if (output.size() > kMaxPayload) break;
  }
  close(pipefd[0]);

  int status = 0;
  waitpid(pid, &status, 0);
  int rc = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return {output, rc};
}

std::string FindPython3() {
  for (const auto& p : {"/usr/bin/python3", "/usr/local/bin/python3"}) {
    if (access(p, X_OK) == 0) return p;
  }
  return "";
}

// ─── Detect skill runtime ───────────────────────────────────
std::pair<std::string, std::string> DetectRuntime(
    const std::string& skill_name) {
  std::string runtime = "python";
  std::string entry_point = skill_name + ".py";

  for (const auto& base : {kSkillsDir, kCustomSkillsDir}) {
    std::string manifest = base + "/" + skill_name + "/manifest.json";
    std::ifstream f(manifest);
    if (!f.is_open()) continue;
    try {
      nlohmann::json j;
      f >> j;
      runtime = j.value("runtime", "python");
      std::string ep;
      if (j.contains("entry_point"))
        ep = j["entry_point"].get<std::string>();
      else if (j.contains("entrypoint"))
        ep = j["entrypoint"].get<std::string>();
      if (!ep.empty()) {
        auto pos = ep.rfind(' ');
        entry_point = (pos != std::string::npos) ? ep.substr(pos + 1) : ep;
      } else {
        if (runtime == "python") entry_point = skill_name + ".py";
        else if (runtime == "node") entry_point = skill_name + ".js";
        else entry_point = skill_name;
      }
    } catch (...) {}
    break;
  }
  return {runtime, entry_point};
}

std::string FindSkillScript(const std::string& skill_name,
                             const std::string& entry_point) {
  for (const auto& base : {kSkillsDir, kCustomSkillsDir}) {
    std::string path = base + "/" + skill_name + "/" + entry_point;
    if (access(path.c_str(), R_OK) == 0) return path;
  }
  return "";
}

std::string ExtractJsonOutput(const std::string& raw) {
  auto output = raw;
  while (!output.empty() && output.back() == '\n') output.pop_back();
  auto pos = output.rfind('\n');
  std::string last_line = (pos != std::string::npos)
                              ? output.substr(pos + 1) : output;
  if (!last_line.empty() &&
      (last_line.front() == '{' || last_line.front() == '[')) {
    return last_line;
  }
  return output;
}

// ─── Peer credential validation ─────────────────────────────
bool ValidatePeer(int client_fd) {
  struct ucred cred;
  socklen_t len = sizeof(cred);
  if (getsockopt(client_fd, SOL_SOCKET, SO_PEERCRED, &cred, &len) != 0) {
    LOG(ERROR) << "getsockopt SO_PEERCRED failed: " << strerror(errno);
    return false;
  }

  std::string exe_link = "/proc/" + std::to_string(cred.pid) + "/exe";
  char exe_path[PATH_MAX] = {};
  ssize_t n = readlink(exe_link.c_str(), exe_path, sizeof(exe_path) - 1);
  if (n <= 0) {
    LOG(ERROR) << "readlink " << exe_link << " failed: " << strerror(errno);
    return false;
  }
  exe_path[n] = '\0';

  std::string basename = exe_path;
  auto slash = basename.rfind('/');
  if (slash != std::string::npos) basename = basename.substr(slash + 1);
  auto del = basename.find(" (deleted)");
  if (del != std::string::npos) basename = basename.substr(0, del);

  for (const auto& allowed : kAllowedCallers) {
    if (basename == allowed) {
      LOG(INFO) << "Peer validated: pid=" << cred.pid << " exe=" << exe_path;
      return true;
    }
  }

  LOG(WARNING) << "Peer rejected: pid=" << cred.pid
               << " exe=" << exe_path << " basename=" << basename;
  return false;
}

// ─── Socket I/O helpers ─────────────────────────────────────
bool RecvExact(int fd, void* buf, size_t n) {
  size_t total = 0;
  while (total < n) {
    ssize_t r = recv(fd, static_cast<char*>(buf) + total,
                     n - total, MSG_WAITALL);
    if (r <= 0) return false;
    total += r;
  }
  return true;
}

bool SendResponse(int fd, const nlohmann::json& resp) {
  std::string payload = resp.dump();
  uint32_t net_len = htonl(payload.size());
  if (write(fd, &net_len, 4) != 4) return false;
  size_t total = 0;
  while (total < payload.size()) {
    ssize_t w = write(fd, payload.data() + total, payload.size() - total);
    if (w <= 0) return false;
    total += w;
  }
  return true;
}

// ─── Command handlers ───────────────────────────────────────

nlohmann::json HandleSkill(const std::string& skill_name,
                            const std::string& args_str) {
  LOG(INFO) << "HandleSkill: " << skill_name;

  auto [runtime, entry_point] = DetectRuntime(skill_name);
  std::string script = FindSkillScript(skill_name, entry_point);
  if (script.empty()) {
    return {{"status", "error"},
            {"output", "Entry point not found for skill: " + skill_name}};
  }

  // For Python skills, try in-process execution via libpython
  if (runtime == "python" && g_py_initialized) {
    LOG(INFO) << "Executing Python skill in-process: " << script;

    // Read the script file
    std::ifstream f(script);
    if (!f.is_open()) {
      return {{"status", "error"},
              {"output", "Cannot open script: " + script}};
    }
    std::string code((std::istreambuf_iterator<char>(f)),
                      std::istreambuf_iterator<char>());

    // Set CLAW_ARGS environment variable
    std::string setup =
        "import os; os.environ['CLAW_ARGS'] = " +
        EscapeShellArg(args_str) + "\n";

    auto [output, rc] = RunPythonCode(setup + code);
    if (rc != 0 && output.empty()) {
      return {{"status", "error"},
              {"output", "Python execution failed (rc=" +
                         std::to_string(rc) + ")"}};
    }
    if (rc != 0) {
      return {{"status", "error"},
              {"output", output.substr(0, 500)}};
    }
    return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
  }

  // Fallback: fork/exec
  std::string cmd;
  if (runtime == "python") {
    std::string python = FindPython3();
    if (python.empty()) {
      return {{"status", "error"}, {"output", "python3 not found"}};
    }
    cmd = "CLAW_ARGS=" + EscapeShellArg(args_str) +
          " " + python + " " + EscapeShellArg(script);
  } else if (runtime == "node") {
    cmd = "CLAW_ARGS=" + EscapeShellArg(args_str) +
          " /usr/bin/node " + EscapeShellArg(script);
  } else {
    cmd = "CLAW_ARGS=" + EscapeShellArg(args_str) +
          " " + EscapeShellArg(script);
  }

  LOG(INFO) << "Exec: runtime=" << runtime << " cmd=" << cmd;
  auto [output, rc] = RunCommand(cmd);

  if (rc != 0) {
    return {{"status", "error"},
            {"output", "exit " + std::to_string(rc) + ": " +
                       output.substr(0, 500)}};
  }
  return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
}

// ─── Code Sandbox connection ──────────────────────────────
int ConnectToCodeSandbox() {
  int s = socket(AF_UNIX, SOCK_STREAM, 0);
  if (s < 0) return -1;

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  addr.sun_path[0] = '\0';
  std::memcpy(addr.sun_path + 1, kSandboxSocketName,
              kSandboxSocketNameLen - 1);

  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path) + 1 +
      kSandboxSocketNameLen - 1;

  if (connect(s, reinterpret_cast<struct sockaddr*>(&addr), addr_len) < 0) {
    close(s);
    return -1;
  }
  return s;
}

/// Forward a JSON request to the code sandbox and return the response.
nlohmann::json ForwardToSandbox(const nlohmann::json& req) {
  int sock = ConnectToCodeSandbox();
  if (sock < 0) {
    LOG(WARNING) << "Code sandbox not available, falling back to host";
    return {};
  }

  std::string payload = req.dump();
  uint32_t net_len = htonl(payload.size());

  if (write(sock, &net_len, 4) != 4) {
    close(sock);
    return {};
  }
  size_t total = 0;
  while (total < payload.size()) {
    ssize_t w = write(sock, payload.data() + total, payload.size() - total);
    if (w <= 0) { close(sock); return {}; }
    total += w;
  }

  // Read response
  uint32_t resp_net_len = 0;
  if (!RecvExact(sock, &resp_net_len, 4)) { close(sock); return {}; }
  uint32_t resp_len = ntohl(resp_net_len);
  if (resp_len > kMaxPayload) { close(sock); return {}; }

  std::vector<char> buf(resp_len);
  if (!RecvExact(sock, buf.data(), resp_len)) { close(sock); return {}; }
  close(sock);

  try {
    return nlohmann::json::parse(std::string(buf.data(), resp_len));
  } catch (...) {
    return {};
  }
}

nlohmann::json HandleExecuteCode(const std::string& code, int timeout) {
  LOG(INFO) << "HandleExecuteCode: " << code.size() << " chars";

  // Try forwarding to code sandbox container first
  nlohmann::json sandbox_req;
  sandbox_req["command"] = "execute_code";
  sandbox_req["code"] = code;
  sandbox_req["timeout"] = timeout;

  auto resp = ForwardToSandbox(sandbox_req);
  if (!resp.empty()) {
    LOG(INFO) << "Code executed in sandbox container";
    return resp;
  }

  // Fallback: run in-process via linked Python
  LOG(INFO) << "Sandbox unavailable, executing in-process";
  if (g_py_initialized) {
    auto [output, rc] = RunPythonCode(code);
    if (rc != 0 && output.empty()) {
      return {{"status", "error"},
              {"output", "Python execution failed (rc=" +
                         std::to_string(rc) + ")"}};
    }
    if (rc != 0) {
      return {{"status", "error"},
              {"output", output.substr(0, 500)}};
    }
    return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
  }

  // Fallback: fork/exec
  std::string python = FindPython3();
  if (python.empty()) {
    return {{"status", "error"}, {"output", "python3 not found"}};
  }
  char tmp_path[] = "/tmp/tizenclaw_dynamic_XXXXXX.py";
  int fd = mkstemps(tmp_path, 3);
  if (fd < 0) {
    return {{"status", "error"}, {"output", "Failed to create temp file"}};
  }
  auto written = write(fd, code.data(), code.size());
  close(fd);
  (void)written;

  auto [output, rc] = RunCommand(python + " " + EscapeShellArg(tmp_path),
                                  kCodeExecTimeout);
  unlink(tmp_path);
  if (rc != 0) {
    return {{"status", "error"},
            {"output", "exit " + std::to_string(rc) + ": " +
                       output.substr(0, 500)}};
  }
  return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
}

nlohmann::json HandleInstallPackage(const std::string& pkg_type,
                                     const std::string& name) {
  LOG(INFO) << "HandleInstallPackage: type=" << pkg_type << " name=" << name;

  nlohmann::json req;
  req["command"] = "install_package";
  req["type"] = pkg_type;
  req["name"] = name;

  auto resp = ForwardToSandbox(req);
  if (!resp.empty()) return resp;

  return {{"status", "error"},
          {"output", "Code sandbox not available for package installation"}};
}

nlohmann::json HandleFileManager(const nlohmann::json& req) {
  namespace fs = std::filesystem;
  std::string operation = req.value("operation", "");
  std::string path = req.value("path", "");

  if (path.empty()) {
    return {{"status", "error"}, {"output", "No path provided"}};
  }

  static const std::vector<std::string> allowed = {
      kCustomSkillsDir,
      kAppDataDir + "/data",
  };

  std::error_code ec;
  std::string real = fs::canonical(path, ec).string();
  if (ec) real = path;

  bool ok = false;
  for (const auto& prefix : allowed) {
    if (real.starts_with(prefix + "/") || real == prefix) {
      ok = true;
      break;
    }
  }
  if (!ok) {
    return {{"status", "error"},
            {"output", "Path outside allowed directories"}};
  }

  LOG(INFO) << "FileManager: op=" << operation << " path=" << path;

  try {
    if (operation == "write_file") {
      std::string content = req.value("content", "");
      fs::create_directories(fs::path(path).parent_path(), ec);
      std::ofstream f(path);
      if (!f.is_open())
        return {{"status", "error"}, {"output", "Failed to write file"}};
      f << content;
      nlohmann::json r = {{"result", "file_written"},
                          {"path", path}, {"size", (int)content.size()}};
      return {{"status", "ok"}, {"output", r.dump()}};
    }
    if (operation == "read_file") {
      if (!fs::is_regular_file(path, ec))
        return {{"status", "error"}, {"output", "File not found: " + path}};
      std::ifstream f(path);
      std::string content((std::istreambuf_iterator<char>(f)),
                           std::istreambuf_iterator<char>());
      nlohmann::json r = {{"result", "file_read"}, {"path", path},
                          {"content", content}, {"size", (int)content.size()}};
      return {{"status", "ok"}, {"output", r.dump()}};
    }
    if (operation == "delete_file") {
      if (!fs::exists(path, ec))
        return {{"status", "error"}, {"output", "Not found: " + path}};
      fs::remove_all(path, ec);
      nlohmann::json r = {{"result", "deleted"}, {"path", path}};
      return {{"status", "ok"}, {"output", r.dump()}};
    }
    if (operation == "list_dir") {
      if (!fs::is_directory(path, ec))
        return {{"status", "error"}, {"output", "Not a directory: " + path}};
      nlohmann::json entries = nlohmann::json::array();
      for (const auto& e : fs::directory_iterator(path, ec)) {
        entries.push_back({
            {"name", e.path().filename().string()},
            {"type", e.is_directory() ? "dir" : "file"},
            {"size", e.is_regular_file() ? (int)e.file_size() : 0},
        });
      }
      nlohmann::json r = {{"result", "listing"},
                          {"path", path}, {"entries", entries}};
      return {{"status", "ok"}, {"output", r.dump()}};
    }
  } catch (const std::exception& e) {
    return {{"status", "error"},
            {"output", std::string("file_manager error: ") + e.what()}};
  }

  return {{"status", "error"},
          {"output", "Unknown operation: " + operation}};
}

nlohmann::json HandleDiag() {
  nlohmann::json diag;
  diag["pid"] = getpid();
  diag["python3_path"] = FindPython3();
  diag["python_embedded"] = g_py_initialized;

  std::vector<std::string> paths = {
      "/usr/bin/python3", kSkillsDir, kCustomSkillsDir,
  };
  for (const auto& p : paths)
    diag["path_exists"][p] = (access(p.c_str(), F_OK) == 0);

  namespace fs = std::filesystem;
  std::error_code ec;
  nlohmann::json skills = nlohmann::json::array();
  if (fs::is_directory(kSkillsDir, ec)) {
    for (const auto& e : fs::directory_iterator(kSkillsDir, ec))
      if (e.is_directory())
        skills.push_back(e.path().filename().string());
  }
  diag["skills"] = skills;

  return {{"status", "ok"}, {"output", diag.dump()}};
}

// ─── Client handler ─────────────────────────────────────────
void HandleClient(int client_fd) {
  if (!ValidatePeer(client_fd)) {
    nlohmann::json resp = {{"status", "error"},
                           {"output", "Permission denied: caller not authorized"}};
    SendResponse(client_fd, resp);
    close(client_fd);
    return;
  }

  while (true) {
    uint32_t net_len = 0;
    if (!RecvExact(client_fd, &net_len, 4)) break;

    uint32_t payload_len = ntohl(net_len);
    if (payload_len > kMaxPayload) {
      LOG(ERROR) << "Payload too large: " << payload_len;
      SendResponse(client_fd, {{"status", "error"},
                               {"output", "Payload too large"}});
      break;
    }

    std::vector<char> buf(payload_len);
    if (!RecvExact(client_fd, buf.data(), payload_len)) break;

    nlohmann::json req;
    try {
      req = nlohmann::json::parse(std::string(buf.data(), payload_len));
    } catch (const std::exception& e) {
      SendResponse(client_fd, {{"status", "error"},
                               {"output", std::string("Bad JSON: ") + e.what()}});
      continue;
    }

    nlohmann::json resp;
    std::string command = req.value("command", "");

    if (command == "diag") {
      resp = HandleDiag();
    } else if (command == "execute_code") {
      std::string code = req.value("code", "");
      int timeout = req.value("timeout", kCodeExecTimeout);
      if (code.empty()) {
        resp = {{"status", "error"}, {"output", "No code provided"}};
      } else {
        resp = HandleExecuteCode(code, timeout);
      }
    } else if (command == "file_manager") {
      resp = HandleFileManager(req);
    } else if (command == "install_package") {
      std::string pkg_type = req.value("type", "pip");
      std::string name = req.value("name", "");
      if (name.empty()) {
        resp = {{"status", "error"}, {"output", "No package name"}};
      } else {
        resp = HandleInstallPackage(pkg_type, name);
      }
    } else {
      std::string skill = req.value("skill", "");
      std::string args = req.value("args", "{}");
      if (skill.empty()) {
        resp = {{"status", "error"}, {"output", "No skill specified"}};
      } else {
        resp = HandleSkill(skill, args);
      }
    }

    if (!SendResponse(client_fd, resp)) break;
  }

  close(client_fd);
}

}  // namespace

// ─── Main ───────────────────────────────────────────────────
int main() {
  LOG(INFO) << "tizenclaw-tool-executor starting (pid=" << getpid() << ")";

  signal(SIGTERM, SignalHandler);
  signal(SIGINT, SignalHandler);
  signal(SIGPIPE, SIG_IGN);

  // Initialize embedded Python (non-fatal if unavailable)
  if (InitPython()) {
    LOG(INFO) << "Embedded Python ready";
  } else {
    LOG(WARNING) << "Embedded Python unavailable, "
                 << "will use fork/exec fallback";
  }

  // Create abstract namespace socket
  int srv = socket(AF_UNIX, SOCK_STREAM, 0);
  if (srv < 0) {
    LOG(ERROR) << "socket() failed: " << strerror(errno);
    return 1;
  }

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  addr.sun_path[0] = '\0';
  std::memcpy(addr.sun_path + 1, kSocketName, kSocketNameLen - 1);

  socklen_t addr_len = offsetof(struct sockaddr_un, sun_path)
                       + 1 + kSocketNameLen - 1;

  if (bind(srv, reinterpret_cast<struct sockaddr*>(&addr), addr_len) < 0) {
    LOG(ERROR) << "bind() failed: " << strerror(errno);
    close(srv);
    return 1;
  }

  if (listen(srv, 10) < 0) {
    LOG(ERROR) << "listen() failed: " << strerror(errno);
    close(srv);
    return 1;
  }

  LOG(INFO) << "Listening on abstract socket: @" << kSocketName;

  while (g_running) {
    int client = accept(srv, nullptr, nullptr);
    if (client < 0) {
      if (errno == EINTR) continue;
      LOG(ERROR) << "accept() failed: " << strerror(errno);
      break;
    }

    std::thread t(HandleClient, client);
    t.detach();
  }

  close(srv);
  LOG(INFO) << "tizenclaw-tool-executor stopped";
  return 0;
}
