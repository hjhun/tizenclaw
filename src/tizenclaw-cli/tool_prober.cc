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

#include "tool_prober.hh"

#include <sys/wait.h>
#include <signal.h>
#include <unistd.h>
#include <fcntl.h>

#include <algorithm>
#include <array>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <sstream>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

struct HelpAttempt {
  std::string suffix;
  int expected_exit;
};

inline bool StartsWith(
    const std::string& s,
    const std::string& prefix) {
  return s.rfind(prefix, 0) == 0;
}

}  // namespace

int ToolProber::RunCapture(
    const std::string& cmd,
    std::string& out, int timeout_sec) {
  // Use fork/exec + pipe for timeout
  // (no 'timeout' command on Tizen)
  int pipefd[2];
  if (pipe(pipefd) < 0) return -1;

  pid_t pid = fork();
  if (pid < 0) {
    close(pipefd[0]);
    close(pipefd[1]);
    return -1;
  }

  if (pid == 0) {
    // Child: redirect stdout+stderr to pipe
    close(pipefd[0]);
    dup2(pipefd[1], STDOUT_FILENO);
    dup2(pipefd[1], STDERR_FILENO);
    close(pipefd[1]);
    execl("/bin/sh", "sh", "-c",
          cmd.c_str(), nullptr);
    _exit(127);
  }

  // Parent: read from pipe with timeout
  close(pipefd[1]);

  // Set non-blocking on the read end
  int flags = fcntl(pipefd[0], F_GETFL, 0);
  fcntl(pipefd[0], F_SETFL, flags | O_NONBLOCK);

  std::array<char, 4096> buffer;
  time_t start = time(nullptr);
  bool timed_out = false;

  while (true) {
    // Check timeout
    if (time(nullptr) - start >= timeout_sec) {
      timed_out = true;
      kill(pid, SIGKILL);
      break;
    }

    ssize_t n = read(pipefd[0], buffer.data(),
                     buffer.size());
    if (n > 0) {
      out.append(buffer.data(), n);
      if (out.size() > 65536) break;
    } else if (n == 0) {
      // EOF
      break;
    } else {
      // EAGAIN — no data yet, brief sleep
      if (errno == EAGAIN || errno == EWOULDBLOCK) {
        usleep(10000);  // 10ms
        continue;
      }
      break;
    }
  }

  close(pipefd[0]);

  int status = 0;
  waitpid(pid, &status, 0);

  if (timed_out) return 124;  // like timeout cmd
  return WIFEXITED(status) ?
      WEXITSTATUS(status) : -1;
}

std::string ToolProber::ExtractDescription(
    const std::string& name,
    const std::string& help_output) {
  if (help_output.empty()) {
    return "System CLI tool: " + name;
  }

  // Try to extract first meaningful line
  std::istringstream iss(help_output);
  std::string line;
  while (std::getline(iss, line)) {
    // Skip empty lines and common headers
    if (line.empty()) continue;
    // Trim
    size_t start = line.find_first_not_of(" \t");
    if (start == std::string::npos) continue;
    line = line.substr(start);
    // Skip lines that are just "Usage:" or
    // the tool name repeated
    if (StartsWith(line, "Usage:") ||
        StartsWith(line, "usage:")) continue;
    if (StartsWith(line, name + " ") ||
        line == name) continue;
    if (StartsWith(line, "Options:") ||
        StartsWith(line, "Commands:")) continue;
    // Found a description-like line
    // Truncate to 200 chars
    if (line.size() > 200) {
      line = line.substr(0, 200) + "...";
    }
    return line;
  }
  return "System CLI tool: " + name;
}

std::string ToolProber::GenerateToolDoc(
    const std::string& name,
    const std::string& binary_path,
    const std::string& help_output) {
  std::ostringstream doc;
  doc << "# " << name << "\n\n";

  // Extract description
  std::string desc =
      ExtractDescription(name, help_output);
  doc << "## Description\n" << desc << "\n\n";

  doc << "## Metadata\n";
  doc << "- **Binary**: `" << binary_path << "`\n";
  doc << "- **Category**: system_cli\n\n";

  doc << "## Usage Details\n";
  doc << "This is a System CLI tool. To use it, invoke `execute_cli` with the `tool_name` set to `" << name << "` and the appropriate `arguments`.\n\n";
  doc << "```bash\n"
      << name << " [options...]\n"
      << "```\n\n";

  // Include full help output
  if (!help_output.empty()) {
    doc << "## Technical Reference & Execution Output\n";
    doc << "Review the help details below to construct valid options and arguments.\n\n";
    doc << "```text\n";
    
    // Allow up to 8KB of output to provide deep context, but truncate if needed
    if (help_output.size() <= 8192) {
      doc << help_output;
    } else {
      doc << help_output.substr(0, 8192);
      doc << "\n... (truncated for brevity)\n";
    }
    
    if (help_output.back() != '\n') {
      doc << "\n";
    }
    doc << "```\n";
  }

  return doc.str();
}

ProbeResult ToolProber::Probe(
    const std::string& binary_path) {
  ProbeResult result;
  namespace fs = std::filesystem;

  // Validate path
  std::error_code ec;
  if (!fs::exists(binary_path, ec)) {
    result.error =
        "Binary not found: " + binary_path;
    return result;
  }

  // Extract tool name from path
  result.name =
      fs::path(binary_path).filename().string();
  if (result.name.empty()) {
    result.error = "Cannot determine tool name";
    return result;
  }

  std::cerr << "Probing " << binary_path
            << " ...\n";

  // Try help variants in order
  std::vector<HelpAttempt> attempts = {
      {"--help", 0},
      {"-h", 0},
      {"help", 0},
      {"--help", 1},  // Some tools exit 1 for help
      {"-h", 1},
  };

  std::string help_output;
  bool got_help = false;
  for (const auto& attempt : attempts) {
    std::string cmd = binary_path + " " +
                      attempt.suffix;
    std::string out;
    int exit_code = RunCapture(cmd, out);
    (void)exit_code;

    if (!out.empty() && out.size() > 10) {
      help_output = out;
      got_help = true;
      std::cerr << "  Got help from: "
                << attempt.suffix << " ("
                << out.size() << " bytes)\n";
      break;
    }
  }

  if (!got_help) {
    // Last resort: run with no args
    std::string out;
    RunCapture(binary_path, out);
    if (!out.empty() && out.size() > 10) {
      help_output = out;
      got_help = true;
      std::cerr
          << "  Got output from bare invocation "
          << "(" << out.size() << " bytes)\n";
    }
  }

  if (!got_help) {
    std::cerr
        << "  Warning: could not get help output. "
        << "Registering with minimal doc.\n";
  } else {
    std::cerr << "  Gathering additional execution outputs...\n";
    std::vector<std::string> additional_cmds = {
        "list", "status", "info", "show", "--version", "get", "--list", "-l"
    };
    std::string additional_context;
    for (const auto& suffix : additional_cmds) {
      std::string cmd = binary_path + " " + suffix;
      std::string out;
      // timeout 2 secs for quick info
      int exit_code = RunCapture(cmd, out, 2);
      if (!out.empty() && exit_code == 0 && out.size() > 5) {
        // limit output to avoid huge payload
        if (out.size() > 2048) {
          out = out.substr(0, 2048) + "\n... (truncated)";
        }
        additional_context += "\n[OUTPUT FOR: " + cmd + "]\n" + out + "\n";
        std::cerr << "  Got additional info from: " << suffix
                  << " (" << out.size() << " bytes)\n";
        if (additional_context.size() > 8192) {
            break; // prevent sending too much data
        }
      }
    }
    if (!additional_context.empty()) {
      help_output += "\n\n--- ACTUAL COMMAND EXECUTION EXAMPLES ---\n" +
                     additional_context;
    }
  }

  result.help_output = help_output;
  result.description =
      ExtractDescription(result.name, help_output);
  result.tool_doc =
      GenerateToolDoc(
          result.name, binary_path, help_output);
  result.success = true;

  std::cerr << "  Tool: " << result.name << "\n"
            << "  Desc: " << result.description
            << "\n";
  return result;
}

}  // namespace cli
}  // namespace tizenclaw
