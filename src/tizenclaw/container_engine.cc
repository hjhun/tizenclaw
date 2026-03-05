#include "container_engine.hh"

#include "../common/logging.hh"
#include <arpa/inet.h>
#include <array>
#include <cerrno>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fcntl.h>
#include <fstream>
#include <json.hpp>
#include <memory>
#include <string>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

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
    : m_initialized(false),
      m_runtime_bin("crun"),
      m_app_data_dir(APP_DATA_DIR),
      m_skills_dir(BuildPaths("skills")),
      m_bundle_dir(BuildPaths("bundles/skills_secure")),
      m_rootfs_tar(BuildPaths("rootfs.tar.gz")),
      m_container_id(kSkillsContainerId),
      m_crun_root(BuildPaths(".crun")) {
}

ContainerEngine::~ContainerEngine() {
    StopSkillsContainer();
}

bool ContainerEngine::Initialize() {
  if (m_initialized) return true;

  LOG(INFO) << "ContainerEngine Initializing...";

  const char* bundled_crun = "/usr/libexec/tizenclaw/crun";
  if (access(bundled_crun, X_OK) == 0) {
    m_runtime_bin = bundled_crun;
    LOG(INFO) << "Using bundled OCI runtime: " << m_runtime_bin;
  }

  // Use RunCommand instead of std::system because /bin/sh is broken in chroot
  auto [out1, r1] = RunCommand(m_runtime_bin + " --version");
  if (r1 == 0) {
    // runtime already set from above detection
  } else {
    auto [out2, r2] = RunCommand("runc --version");
    if (r2 == 0) {
      m_runtime_bin = "runc";
    } else {
      LOG(WARNING) << "Neither crun nor runc found. UDS skill execution will still work.";
      m_runtime_bin = "";
    }
  }
  LOG(INFO) << "Using OCI runtime: " << m_runtime_bin;

  // Prepare crun state directory in a writable path.
  // Default /run/crun may not be writable inside
  // chroot/unshare environments.
  RunCommand("mkdir -p " +
             EscapeShellArg(m_crun_root));
  LOG(INFO) << "crun root dir: " << m_crun_root;

  m_initialized = true;
  return true;
}

std::string ContainerEngine::ExecuteSkill(
    const std::string& skill_name,
    const std::string& arg_str) {
  if (!m_initialized) {
    LOG(ERROR) << "Cannot run skill. "
               << "Engine not initialized.";
    return "{}";
  }

  LOG(ERROR) << "[DEBUG] ExecuteSkill called: skill=" << skill_name
             << " runtime=" << m_runtime_bin;

  // 1st priority: UDS to skill_executor in secure
  // container
  std::string result =
      ExecuteSkillViaSocket(skill_name, arg_str);
  LOG(ERROR) << "[DEBUG] UDS result: len=" << result.length()
             << " content=" << result.substr(0, std::min((size_t)200, result.size()));
  if (!result.empty() && result != "{}") {
    // Check if the result is an error wrapper (e.g., glibc/musl incompatibility)
    try {
      auto rj = nlohmann::json::parse(result);
      if (rj.contains("error")) {
        LOG(ERROR) << "[DEBUG] UDS returned error result, skipping crun, going to host-direct: "
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
    // UDS unavailable — try crun exec before host-direct
    LOG(WARNING) << "UDS unavailable, trying crun "
                 << "exec fallback";
    result =
        ExecuteSkillViaCrun(skill_name, arg_str);
    if (!result.empty() && result != "{}") {
      return result;
    }
  }

  // 3rd priority: host-direct fallback
  LOG(WARNING) << "crun exec failed, trying "
               << "host-direct fallback";
  std::string host_skill_path =
      m_skills_dir + "/" + skill_name + "/" +
      skill_name + ".py";
  if (access(host_skill_path.c_str(), R_OK) != 0) {
    LOG(ERROR) << "Skill not found: "
               << host_skill_path;
    nlohmann::json err;
    err["error"] =
        "Skill script not found: " +
        host_skill_path;
    return err.dump();
  }

  std::string run_cmd =
      "CLAW_ARGS=" + EscapeShellArg(arg_str) +
      " /usr/bin/python3 " +
      EscapeShellArg(host_skill_path);
  LOG(INFO) << "Host-direct skill: " << run_cmd;
  auto [output, rc] = RunCommand(run_cmd);
  if (rc != 0 || output.empty()) {
    LOG(ERROR) << "Host skill failed: rc=" << rc;
    nlohmann::json err;
    err["error"] =
        "Skill failed with exit " +
        std::to_string(rc);
    err["details"] = output.length() > 500
        ? output.substr(0, 500) : output;
    return err.dump();
  }

  return ExtractJsonResult(output);
}

std::string ContainerEngine::ExecuteSkillViaSocket(
    const std::string& skill_name,
    const std::string& arg_str) {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    LOG(WARNING) << "UDS socket() failed: "
                 << strerror(errno);
    return "{}";
  }

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, kSkillSocketPath,
          sizeof(addr.sun_path) - 1);

  if (connect(sock,
              reinterpret_cast<struct sockaddr*>(
                  &addr),
              sizeof(addr)) < 0) {
    LOG(WARNING) << "UDS connect failed: "
                 << strerror(errno);
    close(sock);
    return "{}";
  }

  LOG(ERROR) << "[DEBUG] UDS connected to skill_executor at " << kSkillSocketPath;

  // Build request JSON
  nlohmann::json req;
  req["skill"] = skill_name;
  req["args"] = arg_str;
  std::string payload = req.dump();

  // Send length-prefixed request
  uint32_t net_len = htonl(payload.size());
  if (::write(sock, &net_len, 4) != 4) {
    LOG(ERROR) << "UDS write header failed";
    close(sock);
    return "{}";
  }

  ssize_t total = 0;
  ssize_t len =
      static_cast<ssize_t>(payload.size());
  while (total < len) {
    ssize_t w = ::write(
        sock, payload.data() + total,
        len - total);
    if (w <= 0) {
      LOG(ERROR) << "UDS write body failed";
      close(sock);
      return "{}";
    }
    total += w;
  }

  // Read 4-byte response header
  uint32_t resp_net_len = 0;
  ssize_t hr = ::recv(
      sock, &resp_net_len, 4, MSG_WAITALL);
  if (hr != 4) {
    LOG(ERROR) << "UDS recv header failed";
    close(sock);
    return "{}";
  }

  uint32_t resp_len = ntohl(resp_net_len);
  if (resp_len > 10 * 1024 * 1024) {
    LOG(ERROR) << "UDS response too large: "
               << resp_len;
    close(sock);
    return "{}";
  }

  // Read response body
  std::vector<char> resp_buf(resp_len);
  ssize_t br = ::recv(
      sock, resp_buf.data(), resp_len,
      MSG_WAITALL);
  close(sock);

  if (br != static_cast<ssize_t>(resp_len)) {
    LOG(ERROR) << "UDS recv body incomplete";
    return "{}";
  }

  std::string resp_str(
      resp_buf.data(), resp_len);
  LOG(INFO) << "UDS response (" << resp_len
            << " bytes)";

  // Parse response
  LOG(ERROR) << "[DEBUG] UDS raw response: " << resp_str.substr(0, std::min((size_t)300, resp_str.size()));
  try {
    auto resp = nlohmann::json::parse(resp_str);
    std::string status =
        resp.value("status", "error");
    std::string output =
        resp.value("output", "");
    LOG(ERROR) << "[DEBUG] UDS parsed: status=" << status
               << " output_len=" << output.length()
               << " output_preview=" << output.substr(0, std::min((size_t)200, output.size()));
    if (status == "ok") {
      return output;
    }
    LOG(ERROR) << "Skill executor error: "
               << output;
    nlohmann::json err;
    err["error"] = output;
    return err.dump();
  } catch (const std::exception& e) {
    LOG(ERROR) << "UDS JSON parse error: "
               << e.what();
    return "{}";
  }
}

std::string ContainerEngine::ExecuteSkillViaCrun(
    const std::string& skill_name,
    const std::string& arg_str) {
  if (m_runtime_bin.empty()) {
    return "{}";
  }
  if (!EnsureSkillsContainerRunning()) {
    return "{}";
  }

  std::string claw_env = "CLAW_ARGS=" + arg_str;
  std::string skill_path =
      "/skills/" + skill_name + "/" +
      skill_name + ".py";
  std::string run_cmd =
      CrunCmd("exec --env " +
              EscapeShellArg(claw_env) + " " +
              m_container_id +
              " python3 " +
              EscapeShellArg(skill_path));
  LOG(INFO) << "crun exec skill: " << skill_name;

  auto [output, rc] = RunCommand(run_cmd);
  LOG(INFO) << "crun exec result: rc=" << rc
            << " len=" << output.length();
  if (rc == -1 && output.empty()) {
    return "{}";
  }
  if (rc != 0) {
    LOG(ERROR) << "crun exec failed: " << rc
               << " output: " << output;
    nlohmann::json err;
    err["error"] =
        "crun exec failed with exit " +
        std::to_string(rc);
    err["details"] = output.length() > 500
        ? output.substr(0, 500) : output;
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
  std::string rootfs_dir = m_bundle_dir + "/rootfs";
  std::string marker = m_bundle_dir + "/.extracted";

  std::string prepare_cmd =
      "mkdir -p " + EscapeShellArg(rootfs_dir) + " && " + "if [ ! -f " +
      EscapeShellArg(marker) + " ]; then " + "tar -xzf " +
      EscapeShellArg(m_rootfs_tar) + " -C " + EscapeShellArg(rootfs_dir) +
      " && touch " + EscapeShellArg(marker) + "; fi";

  auto [output, ret] = RunCommand(prepare_cmd);
  if (ret != 0) {
    LOG(ERROR) << "Failed to prepare secure bundle/rootfs. Return: " << ret
               << " output: " << output;
    return false;
  }

  return WriteSkillsConfig();
}

bool ContainerEngine::IsContainerRunning() const {
  std::string check_cmd =
      CrunCmd("state " + m_container_id);
  auto [output, rc] = RunCommand(check_cmd);
  return rc == 0;
}

bool ContainerEngine::StartSkillsContainer() {
  std::string delete_cmd =
      CrunCmd("delete -f " + m_container_id);
  auto [del_out, delete_ret] = RunCommand(delete_cmd);
  if (delete_ret != 0) {
    LOG(WARNING) << "Pre-delete secure container returned: " << delete_ret;
  }

  // Workaround for Tizen emulator: disable cgroup manager if crun supports it
  std::string cgroup_arg = "";
  if (m_runtime_bin.find("crun") != std::string::npos) {
    auto [help_out, help_rc] = RunCommand(
        m_runtime_bin + " run --help");
    if (help_rc == 0 &&
        help_out.find("--cgroup-manager") !=
            std::string::npos) {
      cgroup_arg = " --cgroup-manager=disabled";
    }
  }

  std::string run_cmd =
      "cd " + EscapeShellArg(m_bundle_dir) +
      " && " + CrunCmd("run" + cgroup_arg +
      " -d " + m_container_id);
  auto [run_out, ret] = RunCommand(run_cmd);
  if (ret != 0) {
    LOG(ERROR) << "Failed to start secure skills container. Return: " << ret
               << " output: " << run_out;
    return false;
  }
  return true;
}

void ContainerEngine::StopSkillsContainer() {
  if (!m_initialized || m_runtime_bin.empty()) {
    return;
  }

  std::string stop_cmd =
      CrunCmd("delete -f " + m_container_id);
  auto [output, stop_ret] = RunCommand(stop_cmd);
  if (stop_ret != 0) {
    LOG(WARNING) << "Delete secure container returned: " << stop_ret;
  }
}

bool ContainerEngine::WriteSkillsConfig() const {
  std::string config_file = m_bundle_dir + "/config.json";
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
    "args": ["python3",
             "/skills/skill_executor.py"],
    "env": [
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
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
      {"type": "RLIMIT_AS", "hard": 268435456, "soft": 268435456}
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
      "source": ")" + m_skills_dir + R"(",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/usr",
      "type": "bind",
      "source": "/usr",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/etc",
      "type": "bind",
      "source": "/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/lib64",
      "type": "bind",
      "source": "/lib64",
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
    }
  ],
  "linux": {
    "cgroupsPath": "",
    "namespaces": [
      {"type": "mount"},
      {"type": "pid"},
      {"type": "ipc"}
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
          "rt_sigaction","rt_sigprocmask","rt_sigreturn"
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

std::string ContainerEngine::BuildPaths(
    const std::string& leaf) const {
  if (leaf.empty()) {
    return m_app_data_dir;
  }
  return m_app_data_dir + "/" + leaf;
}

std::string ContainerEngine::EscapeShellArg(
    const std::string& input) const {
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

std::string ContainerEngine::CrunCmd(
    const std::string& subcmd) const {
  return m_runtime_bin + " --root " +
         EscapeShellArg(m_crun_root) +
         " " + subcmd;
}

std::string ContainerEngine::ExtractJsonResult(
    const std::string& raw) {
  std::string trimmed = raw;
  while (!trimmed.empty() &&
         (trimmed.back() == '\n' ||
          trimmed.back() == '\r' ||
          trimmed.back() == ' ')) {
    trimmed.pop_back();
  }
  auto pos = trimmed.rfind('\n');
  if (pos != std::string::npos) {
    std::string last = trimmed.substr(pos + 1);
    if (!last.empty() &&
        (last.front() == '{' ||
         last.front() == '[')) {
      LOG(INFO) << "Extracted JSON from last "
                << "line (skipped "
                << (int)(pos + 1) << " bytes)";
      return last;
    }
  }
  return trimmed;
}

} // namespace tizenclaw
