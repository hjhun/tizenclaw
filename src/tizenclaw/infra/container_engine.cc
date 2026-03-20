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
#include "container_engine.hh"

#include <arpa/inet.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/prctl.h>
#include <sys/wait.h>
#include <unistd.h>

#include <array>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <json.hpp>
#include <memory>
#include <string>
#include <vector>

#include "../../common/logging.hh"

namespace {

std::pair<std::string, std::string> DetectSkillRuntime(
    const std::string& skill_dir,
    const std::string& skill_name) {
  std::string runtime = "python";
  std::string entry_point = skill_name + ".py";

  std::string manifest_path =
      skill_dir + "/" + skill_name + "/manifest.json";
  std::ifstream mf(manifest_path);
  if (mf.is_open()) {
    try {
      nlohmann::json j;
      mf >> j;
      runtime = j.value("runtime", "python");
      // Support both "entry_point" (new) and
      // "entrypoint" (legacy) keys.
      std::string ep;
      if (j.contains("entry_point"))
        ep = j["entry_point"].get<std::string>();
      else if (j.contains("entrypoint"))
        ep = j["entrypoint"].get<std::string>();

      if (!ep.empty()) {
        // Legacy format: "python3 foo.py" — strip
        // runtime prefix, keep only filename.
        auto pos = ep.rfind(' ');
        if (pos != std::string::npos)
          entry_point = ep.substr(pos + 1);
        else
          entry_point = ep;
      } else {
        if (runtime == "python") entry_point = skill_name + ".py";
        else if (runtime == "node") entry_point = skill_name + ".js";
        else entry_point = skill_name;  // native
      }
    } catch (...) {}
  }
  return {runtime, entry_point};
}

}  // namespace

namespace tizenclaw {

// Custom command runner using fork/exec with /bin/bash.
// We cannot use popen() because it invokes /bin/sh, which in the standard
// container is busybox linked against musl libc — but /lib is bind-mounted
// from the host (glibc), making busybox non-functional.
static std::pair<std::string, int> RunCommand(const std::string& cmd) {
  int pipefd[2];
  if (pipe2(pipefd, O_CLOEXEC) == -1) {
    return {"", -1};
  }

  pid_t pid = fork();
  if (pid == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    return {"", -1};
  }

  if (pid == 0) {
    // Child: redirect stdout+stderr to pipe write end
    close(pipefd[0]);
    dup2(pipefd[1], STDOUT_FILENO);
    dup2(pipefd[1], STDERR_FILENO);
    close(pipefd[1]);
    // Use /usr/bin/bash (from host bind-mount, glibc-linked) instead of
    // /bin/bash (from rootfs, musl-linked and broken by host /lib mount).
    execl("/usr/bin/bash", "bash", "-c", cmd.c_str(), nullptr);
    _exit(127);
  }

  // Parent: read from pipe
  close(pipefd[1]);
  std::string output;
  char buffer[256];
  ssize_t n;
  while ((n = read(pipefd[0], buffer, sizeof(buffer) - 1)) > 0) {
    buffer[n] = '\0';
    output += buffer;
  }
  close(pipefd[0]);

  int status = 0;
  waitpid(pid, &status, 0);
  int rc = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return {output, rc};
}

#ifndef APP_DATA_DIR
#define APP_DATA_DIR "/opt/usr/share/tizenclaw"
#endif

namespace {
constexpr const char* kSkillsContainerId = "tizenclaw_skills_secure";
}

ContainerEngine::ContainerEngine()
    : initialized_(false),
      runtime_bin_("crun"),
      app_data_dir_(APP_DATA_DIR),
      skills_dir_(BuildPaths("tools/skills")),
      bundle_dir_(BuildPaths("bundles/skills_secure")),
      rootfs_tar_(BuildPaths("img/rootfs.tar.gz")),
      container_id_(kSkillsContainerId),
      crun_root_(BuildPaths(".crun")) {}

ContainerEngine::~ContainerEngine() { StopSkillsContainer(); }

bool ContainerEngine::Initialize() {
  if (initialized_) return true;

  LOG(INFO) << "ContainerEngine Initializing...";

  const char* bundled_crun = "/usr/libexec/tizenclaw/crun";
  if (access(bundled_crun, X_OK) == 0) {
    runtime_bin_ = bundled_crun;
    LOG(INFO) << "Using bundled OCI runtime: " << runtime_bin_;
  }

  // Use RunCommand instead of std::system because /bin/sh is broken in chroot
  auto [out1, r1] = RunCommand(runtime_bin_ + " --version");
  if (r1 == 0) {
    // runtime already set from above detection
  } else {
    auto [out2, r2] = RunCommand("runc --version");
    if (r2 == 0) {
      runtime_bin_ = "runc";
    } else {
      LOG(WARNING) << "Neither crun nor runc " << "found. UDS skill execution "
                   << "will still work.";
      runtime_bin_ = "";
    }
  }
  LOG(INFO) << "Using OCI runtime: " << runtime_bin_;

  // Prepare crun state directory in a writable path.
  // Default /run/crun may not be writable inside
  // chroot/unshare environments.
  RunCommand("mkdir -p " + EscapeShellArg(crun_root_));
  LOG(INFO) << "crun root dir: " << crun_root_;

  // Ensure data and custom_skills directories exist
  RunCommand("mkdir -p " + EscapeShellArg(app_data_dir_ + "/data"));
  RunCommand("mkdir -p " + EscapeShellArg(app_data_dir_ + "/tools/custom_skills"));

  // Kill any stale container from a previous daemon run.
  // This ensures we always start a fresh container with the
  // latest skill_executor.py after RPM deployment.
  if (!runtime_bin_.empty()) {
    RunCommand(CrunCmd("delete -f " + container_id_));
  }

  CleanupOverlayUsr();

  initialized_ = true;

  // Ensure this process is dumpable so the host crash-worker
  // can read /proc/[pid]/ and generate crash dumps.
  prctl(PR_SET_DUMPABLE, 1);

  // Tool executor is a separate systemd service using an
  // abstract namespace socket — no named socket cleanup
  // needed (abstract sockets auto-clean on process exit).
  // Check if tool-executor is reachable.
  int test_fd = ConnectToToolExecutor();
  if (test_fd >= 0) {
    close(test_fd);
    LOG(INFO) << "Tool executor reachable via abstract "
              << "socket @" << kToolExecutorSocketName;
  } else {
    LOG(WARNING) << "Tool executor not reachable. "
                 << "Ensure tizenclaw-tool-executor.service "
                 << "is running.";
  }

  return true;
}

std::string ContainerEngine::ExecuteSkill(const std::string& skill_name,
                                          const std::string& arg_str) {
  if (!initialized_) {
    LOG(ERROR) << "Cannot run skill. " << "Engine not initialized.";
    return "{}";
  }

  LOG(ERROR) << "[DEBUG] ExecuteSkill called: skill=" << skill_name
             << " runtime=" << runtime_bin_;

  // 1st priority: UDS to skill_executor in secure
  // container
  std::string result = ExecuteSkillViaSocket(skill_name, arg_str);
  LOG(ERROR) << "[DEBUG] UDS result: len=" << result.length() << " content="
             << result.substr(0, std::min((size_t)200, result.size()));
  if (!result.empty() && result != "{}") {
    // Check if the result is an error wrapper
    // (e.g., glibc/musl incompatibility)
    try {
      auto rj = nlohmann::json::parse(result);
      if (rj.contains("error")) {
        LOG(ERROR) << "[DEBUG] UDS returned error " << "result, skipping crun: "
                   << rj["error"].get<std::string>().substr(0, 100);
        // Skip crun exec — if UDS (same container) failed, crun exec will too.
        // Fall through directly to host-direct below.
      } else {
        return result;
      }
    } catch (...) {
      return result;  // Not JSON, return as-is
    }
  } else {
    // UDS unavailable — skip to host-direct fallback
    LOG(WARNING) << "UDS unavailable, trying "
                 << "host-direct fallback";
  }

  // 3rd priority: host-direct fallback
  LOG(WARNING) << "crun exec failed, trying " << "host-direct fallback";

  auto [rt, ep] = DetectSkillRuntime(skills_dir_, skill_name);
  std::string host_skill_path =
      skills_dir_ + "/" + skill_name + "/" + ep;
  if (access(host_skill_path.c_str(), R_OK) != 0) {
    LOG(ERROR) << "Skill not found: " << host_skill_path;
    nlohmann::json err;
    err["error"] = "Skill entry point not found: " + host_skill_path;
    return err.dump();
  }

  std::string run_cmd;
  if (rt == "python") {
    std::string python_bin = FindPython3();
    if (python_bin.empty()) {
      LOG(ERROR) << "No python3 found for " << "host-direct fallback";
      nlohmann::json err;
      err["error"] = "python3 not found on host or rootfs";
      return err.dump();
    }
    run_cmd = "CLAW_ARGS=" + EscapeShellArg(arg_str) +
              " LD_LIBRARY_PATH=/usr/lib:/usr/lib64:/lib:/lib64 " +
              python_bin + " " + EscapeShellArg(host_skill_path);
  } else if (rt == "node") {
    run_cmd = "CLAW_ARGS=" + EscapeShellArg(arg_str) +
              " /usr/bin/node " + EscapeShellArg(host_skill_path);
  } else {
    // native binary
    run_cmd = "CLAW_ARGS=" + EscapeShellArg(arg_str) +
              " " + EscapeShellArg(host_skill_path);
  }
  LOG(INFO) << "Host-direct skill (" << rt << "): " << run_cmd;
  auto [output, rc] = RunCommand(run_cmd);
  if (rc != 0 || output.empty()) {
    LOG(ERROR) << "Host skill failed: rc=" << rc;
    nlohmann::json err;
    err["error"] = "Skill failed with exit " + std::to_string(rc);
    err["details"] = output.length() > 500 ? output.substr(0, 500) : output;
    return err.dump();
  }

  return ExtractJsonResult(output);
}

int ContainerEngine::ConnectToToolExecutor() const {
  int s = socket(AF_UNIX, SOCK_STREAM, 0);
  if (s < 0) return -1;

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  // Abstract namespace: sun_path[0] = '\0', name follows
  addr.sun_path[0] = '\0';
  std::memcpy(addr.sun_path + 1, kToolExecutorSocketName,
              sizeof(kToolExecutorSocketName) - 1);

  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path) + 1 +
      sizeof(kToolExecutorSocketName) - 1;

  if (connect(s, reinterpret_cast<struct sockaddr*>(&addr),
              addr_len) < 0) {
    close(s);
    return -1;
  }
  return s;
}

std::string ContainerEngine::ExecuteToolExecutorCommand(
    const nlohmann::json& req, int timeout_seconds) {
  int sock = ConnectToToolExecutor();
  if (sock < 0) {
    LOG(WARNING) << "Tool executor connect failed";
    return "{}";
  }

  struct timeval tv;
  tv.tv_sec = timeout_seconds > 0 ? timeout_seconds : 10;
  tv.tv_usec = 0;
  setsockopt(sock, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));

  std::string payload = req.dump();
  uint32_t net_len = htonl(payload.size());
  if (::write(sock, &net_len, 4) != 4) {
    LOG(ERROR) << "UDS write header failed";
    close(sock);
    return "{}";
  }

  ssize_t total = 0;
  ssize_t len = static_cast<ssize_t>(payload.size());
  while (total < len) {
    ssize_t w = ::write(sock, payload.data() + total, len - total);
    if (w <= 0) {
      LOG(ERROR) << "UDS write body failed";
      close(sock);
      return "{}";
    }
    total += w;
  }

  uint32_t resp_net_len = 0;
  ssize_t hr = ::recv(sock, &resp_net_len, 4, MSG_WAITALL);
  if (hr != 4) {
    LOG(ERROR) << "UDS recv header failed";
    close(sock);
    return "{}";
  }

  uint32_t resp_len = ntohl(resp_net_len);
  if (resp_len > 10 * 1024 * 1024) {
    LOG(ERROR) << "UDS response too large: " << resp_len;
    close(sock);
    return "{}";
  }

  std::vector<char> resp_buf(resp_len);
  ssize_t br = ::recv(sock, resp_buf.data(), resp_len, MSG_WAITALL);
  close(sock);

  if (br != static_cast<ssize_t>(resp_len)) {
    LOG(ERROR) << "UDS recv body incomplete";
    return "{}";
  }

  std::string resp_str(resp_buf.data(), resp_len);
  try {
    auto resp = nlohmann::json::parse(resp_str);
    std::string status = resp.value("status", "error");
    if (status == "ok") {
      // For session commands, return the full JSON so the agent gets session_id
      if (req.contains("command") &&
          req["command"].get<std::string>().find("session") !=
              std::string::npos) {
        return resp.dump();
      }
      return resp.value("output", "");
    }
    return resp.dump();
  } catch (...) {
    LOG(ERROR) << "UDS JSON parse error";
    return "{}";
  }
}

std::string ContainerEngine::ExecuteSkillViaSocket(
    const std::string& skill_name, const std::string& arg_str) {
  LOG(INFO) << "ExecuteSkillViaSocket: skill=" << skill_name;
  nlohmann::json req;
  req["tool"] = skill_name;
  req["args"] = arg_str;
  return ExecuteToolExecutorCommand(req, 30);
}

std::string ContainerEngine::ExecuteCode(const std::string& code) {
  if (!initialized_) return "{}";
  LOG(INFO) << "ExecuteCode: " << code.size() << " chars";
  nlohmann::json req;
  req["command"] = "execute_code";
  req["code"] = code;
  req["timeout"] = 15;
  return ExecuteToolExecutorCommand(req, 15);
}

std::string ContainerEngine::ExecuteFileOp(const std::string& operation,
                                           const std::string& path,
                                           const std::string& content) {
  if (!initialized_) return "{}";
  LOG(INFO) << "ExecuteFileOp: op=" << operation << " path=" << path;
  nlohmann::json req;
  req["command"] = "file_manager";
  req["operation"] = operation;
  req["path"] = path;
  if (!content.empty()) req["content"] = content;
  return ExecuteToolExecutorCommand(req, 10);
}

std::string ContainerEngine::ExecuteCliTool(
    const std::string& tool_name, const std::string& arguments,
    int timeout_seconds) {
  if (!initialized_) return "{}";
  LOG(INFO) << "ExecuteCliTool: tool=" << tool_name << " args=" << arguments;
  nlohmann::json req;
  req["command"] = "execute_cli";
  req["tool_name"] = tool_name;
  req["arguments"] = arguments;
  req["timeout"] = timeout_seconds;
  return ExecuteToolExecutorCommand(req, timeout_seconds);
}

std::string ContainerEngine::StartCliSession(const std::string& tool_name,
                                             const std::string& arguments,
                                             const std::string& mode,
                                             int timeout_seconds) {
  if (!initialized_) return "{}";
  nlohmann::json req;
  req["command"] = "execute_cli_session";
  req["tool_name"] = tool_name;
  req["arguments"] = arguments;
  req["mode"] = mode;
  req["timeout"] = timeout_seconds;
  return ExecuteToolExecutorCommand(req, timeout_seconds);
}

std::string ContainerEngine::SendToCliSession(const std::string& session_id,
                                              const std::string& input,
                                              int read_timeout_ms) {
  if (!initialized_) return "{}";
  nlohmann::json req;
  req["command"] = "cli_session_send";
  req["session_id"] = session_id;
  req["input"] = input;
  req["read_timeout_ms"] = read_timeout_ms;
  return ExecuteToolExecutorCommand(req, (read_timeout_ms / 1000) + 5);
}

std::string ContainerEngine::ReadCliSession(const std::string& session_id,
                                            int read_timeout_ms) {
  if (!initialized_) return "{}";
  nlohmann::json req;
  req["command"] = "cli_session_read";
  req["session_id"] = session_id;
  req["read_timeout_ms"] = read_timeout_ms;
  return ExecuteToolExecutorCommand(req, (read_timeout_ms / 1000) + 5);
}

std::string ContainerEngine::CloseCliSession(const std::string& session_id) {
  if (!initialized_) return "{}";
  nlohmann::json req;
  req["command"] = "cli_session_close";
  req["session_id"] = session_id;
  return ExecuteToolExecutorCommand(req, 5);
}

std::string ContainerEngine::ExecuteSkillViaCrun(const std::string& skill_name,
                                                 const std::string& arg_str) {
  if (runtime_bin_.empty()) {
    return "{}";
  }
  if (!EnsureSkillsContainerRunning()) {
    return "{}";
  }

  std::string claw_env = "CLAW_ARGS=" + arg_str;

  // Detect runtime from manifest
  auto [rt, ep] = DetectSkillRuntime("/skills", skill_name);
  std::string skill_path = "/skills/" + skill_name + "/" + ep;

  std::string exec_cmd;
  if (rt == "python") {
    exec_cmd = "python3 " + EscapeShellArg(skill_path);
  } else if (rt == "node") {
    exec_cmd = "node " + EscapeShellArg(skill_path);
  } else {
    // native binary
    exec_cmd = EscapeShellArg(skill_path);
  }

  std::string run_cmd =
      CrunCmd("exec --env " + EscapeShellArg(claw_env) + " " + container_id_ +
              " " + exec_cmd);
  LOG(INFO) << "crun exec skill (" << rt << "): " << skill_name;

  auto [output, rc] = RunCommand(run_cmd);
  LOG(INFO) << "crun exec result: rc=" << rc << " len=" << output.length();
  if (rc == -1 && output.empty()) {
    return "{}";
  }
  if (rc != 0) {
    LOG(ERROR) << "crun exec failed: " << rc << " output: " << output;
    nlohmann::json err;
    err["error"] = "crun exec failed with exit " + std::to_string(rc);
    err["details"] = output.length() > 500 ? output.substr(0, 500) : output;
    return err.dump();
  }

  return ExtractJsonResult(output);
}

bool ContainerEngine::EnsureSkillsContainerRunning() {
  if (IsContainerRunning()) {
    return true;
  }

  if (!PrepareSkillsBundle()) {
    return false;
  }

  if (!StartSkillsContainer()) {
    // Auto-restart: force cleanup and try once more
    LOG(WARNING) << "Container start failed. Attempting auto-restart...";
    StopSkillsContainer();
    return StartSkillsContainer();
  }
  return true;
}

bool ContainerEngine::PrepareSkillsBundle() {
  std::string rootfs_dir = bundle_dir_ + "/rootfs";
  std::string marker = bundle_dir_ + "/.extracted";

  std::string prepare_cmd =
      "mkdir -p " + EscapeShellArg(rootfs_dir) + " && " + "if [ ! -f " +
      EscapeShellArg(marker) + " ]; then " + "tar --overwrite -xzf " +
      EscapeShellArg(rootfs_tar_) + " -C " + EscapeShellArg(rootfs_dir) +
      " && touch " + EscapeShellArg(marker) + "; fi";

  auto [output, ret] = RunCommand(prepare_cmd);
  if (ret != 0) {
    LOG(ERROR) << "Failed to prepare secure bundle/rootfs. Return: " << ret
               << " output: " << output;
    return false;
  }

  // Ensure /host_lib directory exists in rootfs for bind mount
  RunCommand("mkdir -p " + EscapeShellArg(rootfs_dir + "/host_lib"));

  if (!WriteSkillsConfig()) {
    return false;
  }
  return PrepareOverlayUsr();
}

bool ContainerEngine::IsContainerRunning() const {
  std::string check_cmd = CrunCmd("state " + container_id_);
  auto [output, rc] = RunCommand(check_cmd);
  return rc == 0;
}

bool ContainerEngine::StartSkillsContainer() {
  // Use the shell script which implements all fallbacks:
  // crun run → runc run → chroot/unshare.
  // Fork a background process so skill_executor.py runs
  // as a daemon; we poll for the UDS socket to appear.
  std::string script =
      "/usr/libexec/tizenclaw/tizenclaw_secure_container.sh";

  if (access(script.c_str(), X_OK) != 0) {
    LOG(ERROR) << "Container start script "
               << "not found: " << script;
    return false;
  }

  pid_t pid = fork();
  if (pid == -1) {
    LOG(ERROR) << "fork() failed for container "
               << "script: " << strerror(errno);
    return false;
  }

  if (pid == 0) {
    // Child: detach and exec the container script.
    setsid();
    // Redirect stdout/stderr to a log file for debugging.
    // Previous /dev/null redirect made crash diagnosis
    // impossible.
    int logfd = open("/tmp/tizenclaw_container_start.log",
                     O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (logfd >= 0) {
      dup2(logfd, STDOUT_FILENO);
      dup2(logfd, STDERR_FILENO);
      close(logfd);
    }
    execl("/usr/bin/bash", "bash", script.c_str(),
          "start", nullptr);
    _exit(127);
  }

  // Parent: don't waitpid — let the child run as daemon.
  LOG(INFO) << "Launched container script "
            << "pid=" << pid;

  // Wait for the tool-executor abstract socket to
  // become connectable (up to 30 seconds).
  for (int i = 0; i < 60; ++i) {
    usleep(500000);  // 500ms
    int fd = ConnectToToolExecutor();
    if (fd >= 0) {
      close(fd);
      LOG(INFO) << "Tool executor socket "
                << "ready after " << (i + 1) * 500
                << "ms";
      return true;
    }
  }

  LOG(WARNING) << "Tool executor socket did "
               << "not appear within 30s";
  return false;
}

void ContainerEngine::StopSkillsContainer() {
  if (!initialized_) {
    return;
  }

  // Call the shell script to stop the container
  // (handles crun delete + overlay cleanup).
  std::string script =
      "/usr/libexec/tizenclaw/tizenclaw_secure_container.sh";
  if (access(script.c_str(), X_OK) == 0) {
    RunCommand(EscapeShellArg(script) + " stop");
  }

  // Abstract namespace sockets auto-clean on process exit.
  // No named socket cleanup needed.

  CleanupOverlayUsr();
}

bool ContainerEngine::PrepareOverlayUsr() {
  std::string merged = bundle_dir_ + "/merged_usr";
  std::string rootfs_usr = bundle_dir_ + "/rootfs/usr";

  RunCommand("mkdir -p " + EscapeShellArg(merged));

  // Check if already mounted
  auto [mnt_out, mnt_rc] =
      RunCommand("mountpoint -q " + EscapeShellArg(merged));
  if (mnt_rc == 0) {
    LOG(INFO) << "OverlayFS for /usr already mounted";
    return true;
  }

  // Read-only overlay: rootfs /usr (priority) + host /usr (fallback)
  // Rootfs (Alpine/musl) libraries must take precedence to avoid
  // glibc/musl symbol mismatches (e.g., libffi __isoc23_sscanf).
  // Host-only libraries (e.g., Tizen CAPI .so) remain accessible.
  std::string overlay_cmd =
      "mount -t overlay overlay -o "
      "lowerdir=" +
      EscapeShellArg(rootfs_usr) + ":/usr " + EscapeShellArg(merged);
  auto [out, rc] = RunCommand(overlay_cmd);
  if (rc != 0) {
    LOG(WARNING) << "OverlayFS mount failed (rc=" << rc
                 << "), falling back to bind mount. "
                 << "output: " << out;
    // Fallback: bind mount host /usr directly
    auto [bind_out, bind_rc] =
        RunCommand("mount --rbind /usr " + EscapeShellArg(merged));
    if (bind_rc != 0) {
      LOG(ERROR) << "Bind mount fallback also failed: " << bind_out;
      return false;
    }
  } else {
    LOG(INFO) << "OverlayFS mounted: /usr + rootfs/usr -> merged_usr";
  }
  return true;
}

void ContainerEngine::CleanupOverlayUsr() {
  std::string merged = bundle_dir_ + "/merged_usr";
  auto [mnt_out, mnt_rc] =
      RunCommand("mountpoint -q " + EscapeShellArg(merged));
  if (mnt_rc == 0) {
    auto [out, rc] = RunCommand("umount " + EscapeShellArg(merged));
    if (rc != 0) {
      LOG(WARNING) << "Failed to umount overlay merged_usr: " << out;
    }
  }
}

bool ContainerEngine::WriteSkillsConfig() const {
  std::string config_file = bundle_dir_ + "/config.json";
  std::ofstream out_conf(config_file);
  if (!out_conf.is_open()) {
    LOG(ERROR) << "Failed to write secure config.json";
    return false;
  }

  std::string config_json = R"({
  "ociVersion": "1.0.2",
  "process": {
    "terminal": false,
    "user": {"uid": 0, "gid": 0},
    "args": ["/usr/bin/python3",
             "/skills/skill_executor.py"],
    "env": [
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
      "LD_LIBRARY_PATH=/lib64:/host_lib:/usr/lib64:/usr/lib:/host_usr_lib:/host_usr_lib64"
    ],
    "cwd": "/",
    "noNewPrivileges": true,
    "capabilities": {
      "bounding": [],
      "effective": [],
      "inheritable": [],
      "permitted": [],
      "ambient": []
    },
    "rlimits": [
      {"type": "RLIMIT_NOFILE", "hard": 256, "soft": 256},
      {"type": "RLIMIT_NPROC", "hard": 64, "soft": 64},
      {"type": "RLIMIT_AS", "hard": 268435456, "soft": 268435456},
      {"type": "RLIMIT_CORE", "hard": 67108864, "soft": 67108864}
    ]
  },
  "root": {
    "path": "rootfs",
    "readonly": true
  },
  "mounts": [
    {
      "destination": "/proc",
      "type": "proc",
      "source": "proc"
    },
    {
      "destination": "/dev",
      "type": "bind",
      "source": "/dev",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/skills",
      "type": "bind",
      "source": ")" + skills_dir_ +
                            R"(",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/data",
      "type": "bind",
      "source": ")" + app_data_dir_ +
                            R"(/data",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/tools/custom_skills",
      "type": "bind",
      "source": ")" + app_data_dir_ +
                            R"(/tools/custom_skills",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/usr",
      "type": "bind",
      "source": ")" + bundle_dir_ +
                            R"(/merged_usr",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/etc",
      "type": "bind",
      "source": "/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/opt/etc",
      "type": "bind",
      "source": "/opt/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/run",
      "type": "bind",
      "source": "/run",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/tmp",
      "type": "bind",
      "source": "/tmp",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/host_lib",
      "type": "bind",
      "source": "/lib",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/opt/usr",
      "type": "bind",
      "source": "/opt/usr",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/opt/usr/share/tizenclaw/tools/cli",
      "type": "bind",
      "source": "/opt/usr/share/tizenclaw/tools/cli",
      "options": ["rbind", "ro"]
    })";

  // Conditionally add /lib64 mount if it exists on host
  namespace fs = std::filesystem;
  std::error_code ec;
  if (fs::is_directory("/lib64", ec)) {
    config_json += R"(,
    {
      "destination": "/lib64",
      "type": "bind",
      "source": "/lib64",
      "options": ["rbind", "ro"]
    })";
  }

  // Conditionally add crash dump directory mount if it
  // exists on host — enables crash-worker to write dumps.
  // The crash handler binary and path vary by device.
  if (fs::is_directory("/opt/usr/share/crash", ec)) {
    config_json += R"(,
    {
      "destination": "/opt/usr/share/crash",
      "type": "bind",
      "source": "/opt/usr/share/crash",
      "options": ["rbind", "rw"]
    })";
  }

  config_json += R"(
  ],
  "linux": {
    "cgroupsPath": "",
    "namespaces": [
      {"type": "mount"}
    ],
    "seccomp": {
      "defaultAction": "SCMP_ACT_ERRNO",
      "architectures": ["SCMP_ARCH_X86_64", "SCMP_ARCH_X86", "SCMP_ARCH_AARCH64"],
      "syscalls": [{
        "names": [
          "read","write","open","close","stat","fstat","lstat",
          "poll","lseek","mmap","mprotect","munmap","brk",
          "ioctl","access","pipe","select","sched_yield",
          "dup","dup2","nanosleep","getpid","socket","connect",
          "sendto","recvfrom","sendmsg","recvmsg","bind","listen",
          "getsockname","getpeername","getsockopt","setsockopt",
          "clone","fork","vfork","execve","exit","wait4",
          "kill","uname","fcntl","flock","fsync","fdatasync",
          "truncate","ftruncate","getdents","getcwd","chdir",
          "mkdir","rmdir","creat","link","unlink","symlink",
          "readlink","chmod","chown","lchown","umask",
          "gettimeofday","getrlimit","getrusage","sysinfo",
          "times","getuid","getgid","setuid","setgid",
          "geteuid","getegid","getppid","getpgrp","setsid",
          "getgroups","setgroups","sigaltstack","madvise",
          "shmget","shmat","shmctl","shmdt",
          "clock_gettime","clock_getres","clock_nanosleep",
          "exit_group","epoll_wait","epoll_ctl","tgkill",
          "openat","mkdirat","fchownat","fstatat",
          "unlinkat","renameat","linkat","symlinkat",
          "readlinkat","fchmodat","faccessat","futex",
          "set_robust_list","get_robust_list",
          "epoll_create1","pipe2","dup3","accept4",
          "prlimit64","getrandom","memfd_create",
          "statx","clone3","close_range","rseq",
          "newfstatat","accept","shutdown","fchmod",
          "rt_sigaction","rt_sigprocmask","rt_sigreturn",
          "prctl","getrlimit"
        ],
        "action": "SCMP_ACT_ALLOW"
      }]
    },
    "maskedPaths": [
      "/proc/acpi",
      "/proc/kcore",
      "/proc/keys",
      "/proc/latency_stats",
      "/proc/timer_list",
      "/proc/timer_stats",
      "/proc/sched_debug",
      "/sys/firmware"
    ],
    "readonlyPaths": [
      "/proc/asound",
      "/proc/bus",
      "/proc/fs",
      "/proc/irq",
      "/proc/sys",
      "/proc/sysrq-trigger"
    ]
  }
})";
  out_conf << config_json;
  out_conf.close();
  return true;
}

std::string ContainerEngine::BuildPaths(const std::string& leaf) const {
  if (leaf.empty()) {
    return app_data_dir_;
  }
  return app_data_dir_ + "/" + leaf;
}

std::string ContainerEngine::EscapeShellArg(const std::string& input) const {
  std::string output = "'";
  for (char c : input) {
    if (c == '\'') {
      output += "'\\''";
    } else {
      output += c;
    }
  }
  output += "'";
  return output;
}

std::string ContainerEngine::CrunCmd(const std::string& subcmd) const {
  return runtime_bin_ + " --root " + EscapeShellArg(crun_root_) + " " + subcmd;
}

std::string ContainerEngine::FindPython3() const {
  // 1st: host system python3
  if (access("/usr/bin/python3", X_OK) == 0) {
    return "/usr/bin/python3";
  }

  // 2nd: rootfs python3 via musl dynamic linker.
  // Alpine (musl) python3 can be executed on any Linux kernel by
  // invoking musl's ld directly as the ELF interpreter.
  namespace fs = std::filesystem;

  // Resolve python3 symlink (Alpine: python3 -> python3.12)
  // musl ld requires the actual ELF binary path, not a symlink.
  std::string rootfs_python = bundle_dir_ + "/rootfs/usr/bin/python3";
  std::error_code ec;
  fs::path resolved = fs::canonical(rootfs_python, ec);
  if (ec || !fs::exists(resolved, ec)) {
    LOG(WARNING) << "rootfs python3 not found: " << rootfs_python;
    return "";
  }
  std::string real_python = resolved.string();

  // Find ld-musl-*.so.1 in rootfs /lib
  std::string rootfs_lib = bundle_dir_ + "/rootfs/lib";
  for (const auto& entry : fs::directory_iterator(rootfs_lib, ec)) {
    std::string name = entry.path().filename().string();
    if (name.find("ld-musl-") == 0 &&
        name.find(".so.1") != std::string::npos) {
      std::string musl_ld = entry.path().string();
      std::string rootfs_usr_lib =
          bundle_dir_ + "/rootfs/usr/lib";
      LOG(INFO) << "Using rootfs python3 via " << "musl ld: " << musl_ld;
      return musl_ld + " --library-path " +
             EscapeShellArg(rootfs_usr_lib + ":/usr/lib:/lib") +
             " " + real_python;
    }
  }

  return "";
}

std::string ContainerEngine::ExtractJsonResult(const std::string& raw) {
  std::string trimmed = raw;
  while (!trimmed.empty() &&
         (trimmed.back() == '\n' || trimmed.back() == '\r' ||
          trimmed.back() == ' ')) {
    trimmed.pop_back();
  }
  auto pos = trimmed.rfind('\n');
  if (pos != std::string::npos) {
    std::string last = trimmed.substr(pos + 1);
    if (!last.empty() && (last.front() == '{' || last.front() == '[')) {
      LOG(INFO) << "Extracted JSON from last " << "line (skipped "
                << (int)(pos + 1) << " bytes)";
      return last;
    }
  }
  return trimmed;
}

}  // namespace tizenclaw
