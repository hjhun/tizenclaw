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

#include "tool_handler.hh"

#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <fstream>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_TOOL_EXECUTOR"

#include "../common/logging.hh"
#include "../tizenclaw/core/skill_manifest.hh"

namespace tizenclaw {
namespace tool_executor {

namespace {

const std::string kAppDataDir = "/opt/usr/share/tizenclaw";
const std::vector<std::string> kToolSearchPaths = {
    "/opt/usr/share/tizen-tools/skills",
    "/opt/usr/share/tizen-tools/custom_skills",
    "/opt/usr/share/tizen-tools/cli",
};
constexpr int kExecTimeout = 30;
constexpr size_t kMaxPayload = 10 * 1024 * 1024;

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

  // Use poll to enforce timeout on pipe read
  struct pollfd pfd = {pipefd[0], POLLIN, 0};
  auto deadline = std::chrono::steady_clock::now() +
                  std::chrono::seconds(timeout_sec);
  bool timed_out = false;

  while (true) {
    auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(
        deadline - std::chrono::steady_clock::now());
    int ms = std::max(0, static_cast<int>(remaining.count()));
    if (ms == 0) { timed_out = true; break; }

    int pr = poll(&pfd, 1, ms);
    if (pr < 0) {
      if (errno == EINTR) continue;
      break;
    }
    if (pr == 0) { timed_out = true; break; }

    ssize_t n = read(pipefd[0], buf, sizeof(buf));
    if (n <= 0) break;
    output.append(buf, n);
    if (output.size() > kMaxPayload) break;
  }
  close(pipefd[0]);

  if (timed_out) {
    kill(pid, SIGKILL);
    waitpid(pid, nullptr, 0);
    return {"Execution timed out after " +
            std::to_string(timeout_sec) + "s", -1};
  }

  int status = 0;
  waitpid(pid, &status, 0);
  int rc = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return {output, rc};
}

}  // namespace

ToolHandler::ToolHandler(PythonEngine& python_engine)
    : python_engine_(python_engine) {}

std::pair<std::string, std::string> ToolHandler::DetectRuntime(
    const std::string& tool_name) {
  std::string runtime = "python";
  std::string entry_point = tool_name + ".py";

  for (const auto& base : kToolSearchPaths) {
    std::string tool_dir = base + "/" + tool_name;

    // Use SkillManifest for unified loading
    // (SKILL.md > manifest.json)
    nlohmann::json j = SkillManifest::Load(tool_dir);
    if (j.empty()) {
      LOG(DEBUG) << "No manifest in: " << tool_dir;
      continue;
    }
    LOG(DEBUG) << "Manifest found: " << tool_dir;

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
      if (runtime == "python") entry_point = tool_name + ".py";
      else if (runtime == "node") entry_point = tool_name + ".js";
      else entry_point = tool_name;
    }
    break;
  }
  LOG(DEBUG) << "DetectRuntime: runtime=" << runtime
             << " entry_point=" << entry_point;
  return {runtime, entry_point};
}

std::string ToolHandler::FindToolScript(const std::string& tool_name,
                                          const std::string& entry_point) {
  for (const auto& base : kToolSearchPaths) {
    std::string path = base + "/" + tool_name + "/" + entry_point;
    if (access(path.c_str(), R_OK) == 0) {
      LOG(DEBUG) << "Tool script found: " << path;
      return path;
    }
    LOG(DEBUG) << "Tool script not at: " << path;
  }
  LOG(DEBUG) << "Tool script not found for: " << tool_name;
  return "";
}

nlohmann::json ToolHandler::HandleTool(const std::string& tool_name,
                                         const std::string& args_str) {
  LOG(INFO) << "HandleTool: " << tool_name;

  auto [runtime, entry_point] = DetectRuntime(tool_name);
  std::string script = FindToolScript(tool_name, entry_point);
  if (script.empty()) {
    return {{"status", "error"},
            {"output", "Entry point not found for tool: " + tool_name}};
  }

  // Always use fork/exec for tool scripts.
  // In-process Python is NOT used here because tool scripts may import
  // C extension modules that are incompatible with embedded Python and
  // would crash the tool executor process.
  std::string cmd;
  if (runtime == "python") {
    std::string python = PythonEngine::FindPython3();
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
  LOG(DEBUG) << "RunCommand result: rc=" << rc
             << " output_len=" << output.size();

  if (rc != 0) {
    return {{"status", "error"},
            {"output", "exit " + std::to_string(rc) + ": " +
                       output.substr(0, 500)}};
  }
  if (output.empty()) {
    LOG(WARNING) << "Tool " << tool_name << " returned empty output (rc=0)";
  }
  return {{"status", "ok"}, {"output", ExtractJsonOutput(output)}};
}

}  // namespace tool_executor
}  // namespace tizenclaw
