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

#ifndef TIZENCLAW_CLI_TOOL_PROBER_HH_
#define TIZENCLAW_CLI_TOOL_PROBER_HH_

#include <string>

namespace tizenclaw {
namespace cli {

// Probes a system executable to extract its
// capabilities. Runs the binary with --help/-h
// and captures the output, then generates a
// tool.md document and description for
// registration.
struct ProbeResult {
  bool success = false;
  std::string name;         // tool name (basename)
  std::string description;  // short description
  std::string tool_doc;     // generated tool.md
  std::string help_output;  // raw help output
  std::string error;        // error message
};

class ToolProber {
 public:
  // Probe the executable at the given path.
  // Tries --help, -h, help in order.
  static ProbeResult Probe(
      const std::string& binary_path);

 private:
  // Run a command and capture stdout+stderr.
  // Returns exit code, output in `out`.
  static int RunCapture(
      const std::string& cmd,
      std::string& out, int timeout_sec = 5);

  // Generate a tool.md document from help text.
  static std::string GenerateToolDoc(
      const std::string& name,
      const std::string& binary_path,
      const std::string& help_output);

  // Extract a short description from help text.
  static std::string ExtractDescription(
      const std::string& name,
      const std::string& help_output);
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_TOOL_PROBER_HH_
