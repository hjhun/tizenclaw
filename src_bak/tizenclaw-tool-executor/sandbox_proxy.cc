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

#include "sandbox_proxy.hh"

#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <vector>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_TOOL_EXECUTOR"

#include "../common/logging.hh"

namespace tizenclaw {
namespace tool_executor {

namespace {

constexpr size_t kMaxPayload = 10 * 1024 * 1024;
constexpr int kCodeExecTimeout = 15;

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

std::string EscapeShellArg(const std::string& s) {
  std::string out = "'";
  for (char c : s) {
    if (c == '\'') out += "'\\''";
    else out += c;
  }
  out += "'";
  return out;
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

}  // namespace

constexpr const char SandboxProxy::kSandboxSocketName[];

SandboxProxy::SandboxProxy(PythonEngine& python_engine)
    : python_engine_(python_engine) {}

int SandboxProxy::ConnectToSandbox() {
  LOG(DEBUG) << "Connecting to sandbox socket: @" << kSandboxSocketName;
  int s = socket(AF_UNIX, SOCK_STREAM, 0);
  if (s < 0) return -1;

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  addr.sun_path[0] = '\0';
  std::memcpy(addr.sun_path + 1, kSandboxSocketName,
              sizeof(kSandboxSocketName) - 1);

  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path) + 1 +
      sizeof(kSandboxSocketName) - 1;

  if (connect(s, reinterpret_cast<struct sockaddr*>(&addr), addr_len) < 0) {
    LOG(DEBUG) << "Sandbox connect failed: " << strerror(errno);
    close(s);
    return -1;
  }
  LOG(DEBUG) << "Sandbox connected: fd=" << s;
  return s;
}

nlohmann::json SandboxProxy::ForwardRequest(const nlohmann::json& req) {
  int sock = ConnectToSandbox();
  if (sock < 0) {
    LOG(WARNING) << "Code sandbox not available";
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

nlohmann::json SandboxProxy::HandleExecuteCode(const std::string& code,
                                                 int timeout) {
  LOG(INFO) << "HandleExecuteCode: " << code.size() << " chars";

  // Try sandbox container first
  nlohmann::json sandbox_req;
  sandbox_req["command"] = "execute_code";
  sandbox_req["code"] = code;
  sandbox_req["timeout"] = timeout;

  auto resp = ForwardRequest(sandbox_req);
  if (!resp.empty()) {
    LOG(INFO) << "Code executed in sandbox container";
    return resp;
  }

  // Fallback: in-process Python
  LOG(INFO) << "Sandbox unavailable, executing in-process";
  if (python_engine_.IsInitialized()) {
    LOG(DEBUG) << "Using in-process Python fallback";
    auto [output, rc] = python_engine_.RunCode(code);
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
  LOG(DEBUG) << "Using fork/exec Python fallback";
  std::string python = PythonEngine::FindPython3();
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

  // Simple fork/exec for code
  std::string cmd = python + " " + EscapeShellArg(tmp_path);
  FILE* fp = popen(cmd.c_str(), "r");
  std::string output;
  if (fp) {
    char buf[4096];
    while (fgets(buf, sizeof(buf), fp)) output += buf;
    int rc = pclose(fp);
    unlink(tmp_path);
    if (rc != 0) {
      return {{"status", "error"},
              {"output", "exit " + std::to_string(rc) + ": " +
                         output.substr(0, 500)}};
    }
    return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
  }
  unlink(tmp_path);
  return {{"status", "error"}, {"output", "popen failed"}};
}

nlohmann::json SandboxProxy::HandleInstallPackage(const std::string& type,
                                                    const std::string& name) {
  LOG(INFO) << "HandleInstallPackage: type=" << type << " name=" << name;

  nlohmann::json req;
  req["command"] = "install_package";
  req["type"] = type;
  req["name"] = name;

  auto resp = ForwardRequest(req);
  if (!resp.empty()) return resp;

  return {{"status", "error"},
          {"output", "Code sandbox not available for package installation"}};
}

}  // namespace tool_executor
}  // namespace tizenclaw
