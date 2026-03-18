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

#include <arpa/inet.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <poll.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <filesystem>
#include <string>
#include <thread>
#include <vector>

#include <json.hpp>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_TOOL_EXECUTOR"

#include "../common/logging.hh"

#include "file_manager.hh"
#include "peer_validator.hh"
#include "python_engine.hh"
#include "sandbox_proxy.hh"
#include "tool_handler.hh"

namespace {

// ─── Constants ──────────────────────────────────────────────
constexpr const char kSocketName[] = "tizenclaw-tool-executor.sock";
constexpr size_t kSocketNameLen = sizeof(kSocketName);
constexpr size_t kMaxPayload = 10 * 1024 * 1024;
constexpr int kCodeExecTimeout = 15;

const std::string kAppDataDir = "/opt/usr/share/tizenclaw";
const std::string kToolsDir = kAppDataDir + "/tools/skills";

volatile sig_atomic_t g_running = 1;
void SignalHandler(int) { g_running = 0; }

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

// ─── Shell Argument Escaping ────────────────────────────────
std::string EscapeShellArg(const std::string& s) {
  std::string out = "'";
  for (char c : s) {
    if (c == '\'') out += "'\\''";
    else out += c;
  }
  out += "'";
  return out;
}

// ─── RAII fd wrapper ────────────────────────────────────────
class UniqueFd {
 public:
  explicit UniqueFd(int fd = -1) : fd_(fd) {}
  ~UniqueFd() { if (fd_ >= 0) close(fd_); }
  UniqueFd(const UniqueFd&) = delete;
  UniqueFd& operator=(const UniqueFd&) = delete;
  UniqueFd(UniqueFd&& o) noexcept : fd_(o.fd_) { o.fd_ = -1; }
  int Get() const { return fd_; }
  int Release() { int f = fd_; fd_ = -1; return f; }
 private:
  int fd_;
};

// ─── Diagnostics ────────────────────────────────────────────
nlohmann::json HandleDiag(
    const tizenclaw::tool_executor::PythonEngine& python_engine) {
  nlohmann::json diag;
  diag["pid"] = getpid();
  diag["python3_path"] =
      tizenclaw::tool_executor::PythonEngine::FindPython3();
  diag["python_embedded"] = python_engine.IsInitialized();

  namespace fs = std::filesystem;
  std::error_code ec;
  nlohmann::json tools = nlohmann::json::array();
  if (fs::is_directory(kToolsDir, ec)) {
    for (const auto& e : fs::directory_iterator(kToolsDir, ec))
      if (e.is_directory())
        tools.push_back(e.path().filename().string());
  }
  diag["tools"] = tools;

  return {{"status", "ok"}, {"output", diag.dump()}};
}

// ─── Client handler ─────────────────────────────────────────
void HandleClient(
    int raw_fd,
    tizenclaw::tool_executor::PeerValidator& validator,
    tizenclaw::tool_executor::PythonEngine& python_engine,
    tizenclaw::tool_executor::ToolHandler& tool_handler,
    tizenclaw::tool_executor::SandboxProxy& sandbox_proxy,
    tizenclaw::tool_executor::FileManager& file_manager) {
  UniqueFd client_fd(raw_fd);
  LOG(DEBUG) << "New client fd=" << client_fd.Get();
  if (!validator.Validate(client_fd.Get())) {
    LOG(WARNING) << "Rejecting unauthenticated peer";
    nlohmann::json resp = {
        {"status", "error"},
        {"output", "Permission denied: caller not authorized"}};
    SendResponse(client_fd.Get(), resp);
    return;
  }

  while (true) {
    uint32_t net_len = 0;
    if (!RecvExact(client_fd.Get(), &net_len, 4)) break;

    uint32_t payload_len = ntohl(net_len);
    if (payload_len > kMaxPayload) {
      LOG(ERROR) << "Payload too large: " << payload_len;
      SendResponse(client_fd.Get(), {{"status", "error"},
                               {"output", "Payload too large"}});
      break;
    }

    std::vector<char> buf(payload_len);
    if (!RecvExact(client_fd.Get(), buf.data(), payload_len)) break;

    nlohmann::json req;
    try {
      req = nlohmann::json::parse(std::string(buf.data(), payload_len));
    } catch (const std::exception& e) {
      SendResponse(client_fd.Get(), {{"status", "error"},
                               {"output", std::string("Bad JSON: ") +
                                          e.what()}});
      continue;
    }

    nlohmann::json resp;
    std::string command = req.value("command", "");

    LOG(INFO) << "Command: " << command;
    LOG(DEBUG) << "Request payload size=" << payload_len << " cmd=" << command;

    if (command == "diag") {
      LOG(DEBUG) << "Handling diag command";
      resp = {{"status", "ok"},
              {"output", "tool-executor alive, pid=" +
                         std::to_string(getpid())}};
    } else if (command == "execute_code") {
      std::string code = req.value("code", "");
      int timeout = req.value("timeout", kCodeExecTimeout);
      LOG(DEBUG) << "execute_code: code_len=" << code.size()
                 << " timeout=" << timeout;
      if (code.empty()) {
        resp = {{"status", "error"}, {"output", "No code provided"}};
      } else {
        resp = sandbox_proxy.HandleExecuteCode(code, timeout);
      }
    } else if (command == "file_manager") {
      std::string op = req.value("operation", "");
      std::string path = req.value("path", "");
      LOG(DEBUG) << "file_manager: op=" << op << " path=" << path;
      resp = file_manager.Handle(req);
    } else if (command == "install_package") {
      std::string pkg_type = req.value("type", "pip");
      std::string name = req.value("name", "");
      if (name.empty()) {
        resp = {{"status", "error"}, {"output", "No package name"}};
      } else {
        resp = sandbox_proxy.HandleInstallPackage(pkg_type, name);
      }
    } else if (command == "execute_cli") {
      std::string cli_tool = req.value("tool_name", "");
      std::string cli_args = req.value("arguments", "");
      int cli_timeout = req.value("timeout", 10);
      LOG(DEBUG) << "execute_cli: tool=" << cli_tool << " args=" << cli_args
                 << " timeout=" << cli_timeout;
      if (cli_tool.empty()) {
        resp = {{"status", "error"}, {"output", "No tool_name"}};
      } else {
        // Resolve to /usr/bin/<tool_name> and execute
        std::string bin_path = "/usr/bin/" + cli_tool;
        namespace fs = std::filesystem;
        std::error_code fec;
        if (!fs::exists(bin_path, fec)) {
          resp = {{"status", "error"},
                  {"output", "CLI binary not found: " + bin_path}};
        } else {
          std::string cmd = bin_path + " " + EscapeShellArg(cli_args) + " 2>&1";
          LOG(INFO) << "execute_cli: " << cmd
                    << " (timeout=" << cli_timeout << "s)";

          std::string output;
          FILE* pipe = popen(cmd.c_str(), "r");
          if (!pipe) {
            resp = {{"status", "error"},
                    {"output", "popen failed for " + cli_tool}};
          } else {
            char read_buf[4096];
            while (fgets(read_buf, sizeof(read_buf),
                         pipe) != nullptr) {
              output += read_buf;
            }
            int exit_status = pclose(pipe);
            int exit_code = WIFEXITED(exit_status)
                                ? WEXITSTATUS(exit_status)
                                : -1;

            // Trim trailing newlines
            while (!output.empty() && output.back() == '\n') {
              output.pop_back();
            }

            nlohmann::json result;
            result["tool"] = cli_tool;
            result["exit_code"] = exit_code;
            result["output"] = output;
            resp = {{"status", "ok"},
                    {"output", result.dump()}};
          }
        }
      }
    } else {
      // Default: tool execution (renamed from "skill")
      std::string tool = req.value("tool", "");
      std::string args = req.value("args", "{}");
      LOG(DEBUG) << "tool_execution: tool=" << tool << " args=" << args;
      if (tool.empty()) {
        resp = {{"status", "error"}, {"output", "No tool specified"}};
      } else {
        resp = tool_handler.HandleTool(tool, args);
      }
    }

    if (!SendResponse(client_fd.Get(), resp)) break;
  }

  // UniqueFd automatically closes fd on scope exit
  LOG(DEBUG) << "Client fd=" << client_fd.Get() << " disconnected";
}

}  // namespace

// ─── Main ───────────────────────────────────────────────────
int main() {
  LOG(INFO) << "tizenclaw-tool-executor starting (pid=" << getpid() << ")";
  LOG(DEBUG) << "Startup: Initializing signal handlers and components.";

  signal(SIGTERM, SignalHandler);
  signal(SIGINT, SignalHandler);
  signal(SIGPIPE, SIG_IGN);

  // Construct components
  tizenclaw::tool_executor::PeerValidator validator(
      {"tizenclaw", "tizenclaw-cli"});
  tizenclaw::tool_executor::PythonEngine python_engine;
  tizenclaw::tool_executor::ToolHandler tool_handler(python_engine);
  tizenclaw::tool_executor::SandboxProxy sandbox_proxy(python_engine);
  tizenclaw::tool_executor::FileManager file_manager;

  // Initialize embedded Python (non-fatal)
  if (python_engine.Initialize()) {
    LOG(INFO) << "Embedded Python ready";
  } else {
    LOG(WARNING) << "Embedded Python unavailable, "
                 << "will use fork/exec fallback";
  }

  // Check for systemd socket activation (LISTEN_FDS/LISTEN_PID)
  int srv = -1;
  const char* listen_fds_env = getenv("LISTEN_FDS");
  const char* listen_pid_env = getenv("LISTEN_PID");

  if (listen_fds_env && listen_pid_env) {
    try {
      int lpid = std::stoi(listen_pid_env);
      int lfds = std::stoi(listen_fds_env);
      if (lpid == getpid() && lfds >= 1) {
        // Socket-activated: fd 3 is SD_LISTEN_FDS_START
        srv = 3;
        LOG(INFO) << "Using systemd socket activation (fd=" << srv << ")";
      }
    } catch (const std::exception& e) {
      LOG(WARNING) << "Invalid LISTEN_PID/LISTEN_FDS env: " << e.what();
    }
  }

  if (srv < 0) {
    // Manual start: create abstract namespace socket
    srv = socket(AF_UNIX, SOCK_STREAM, 0);
    if (srv < 0) {
      LOG(ERROR) << "socket() failed: " << strerror(errno);
      return 1;
    }

    struct sockaddr_un addr;
    std::memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    addr.sun_path[0] = '\0';
    std::memcpy(addr.sun_path + 1, kSocketName,
                kSocketNameLen - 1);

    socklen_t addr_len = offsetof(struct sockaddr_un, sun_path)
                         + 1 + kSocketNameLen - 1;

    if (bind(srv,
             reinterpret_cast<struct sockaddr*>(&addr),
             addr_len) < 0) {
      LOG(ERROR) << "bind() failed: " << strerror(errno);
      close(srv);
      return 1;
    }

    if (listen(srv, 128) < 0) {
      LOG(ERROR) << "listen() failed: " << strerror(errno);
      close(srv);
      return 1;
    }
  }

  LOG(INFO) << "Listening on abstract socket: @" << kSocketName;

  while (g_running) {
    struct pollfd pfd = {srv, POLLIN, 0};
    int ret = poll(&pfd, 1, 1000);  // 1s timeout for shutdown check
    if (ret < 0) {
      if (errno == EINTR) continue;
      LOG(ERROR) << "poll() failed: " << strerror(errno);
      break;
    }
    if (ret == 0) continue;  // timeout, re-check g_running

    int client = accept(srv, nullptr, nullptr);
    if (client < 0) {
      if (errno == EINTR) continue;
      LOG(ERROR) << "accept() failed: " << strerror(errno);
      break;
    }
    LOG(DEBUG) << "Accepted new client connection, fd=" << client;

    std::thread t(HandleClient, client,
                  std::ref(validator),
                  std::ref(python_engine),
                  std::ref(tool_handler),
                  std::ref(sandbox_proxy),
                  std::ref(file_manager));
    t.detach();
  }

  close(srv);
  LOG(INFO) << "tizenclaw-tool-executor stopped";
  return 0;
}
