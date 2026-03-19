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

#ifndef SYSTEM_CLI_ADAPTER_HH_
#define SYSTEM_CLI_ADAPTER_HH_

#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

// Per-tool configuration from system_cli_config.json
struct SystemCliToolConfig {
  std::string binary_path;                  // e.g., "/usr/bin/aul_test"
  std::vector<std::string> blocked_args;    // blocked argument patterns
  int timeout_seconds = 10;                 // execution timeout
  std::string side_effect;                  // "none"|"reversible"|"irreversible"
  std::string description;                  // short description
};

// Manages system-level CLI tools from /usr/bin that are
// explicitly whitelisted via system_cli_config.json.
//
// Unlike CliPluginManager (which manages TPK-packaged CLI tools),
// this adapter handles pre-installed system binaries that don't
// require package installation. Each tool must be:
//   1. Listed in system_cli_config.json (whitelist)
//   2. Present at the configured binary path
//   3. Accompanied by a .tool.md descriptor for LLM documentation
//
// Security: Only whitelisted tools are accessible. Per-tool
// blocked_args patterns prevent dangerous argument combinations.
class SystemCliAdapter {
 public:
  static SystemCliAdapter& GetInstance();

  // Load config and scan for available tools
  bool Initialize(const std::string& config_path);
  void Shutdown();

  // Check if a tool is a registered system CLI tool
  bool HasTool(const std::string& name) const;

  // Resolve tool name to full binary path (empty if not found)
  std::string Resolve(const std::string& name) const;

  // Validate arguments against blocked_args patterns.
  // Returns empty string if valid, error message if blocked.
  std::string ValidateArguments(const std::string& name,
                                const std::string& arguments) const;

  // Get all registered system CLI tool names
  std::vector<std::string> GetToolNames() const;

  // Get tool.md documentation map (name → content)
  std::map<std::string, std::string> GetToolDocs() const;

  // Get timeout for a tool (default: 10s)
  int GetTimeout(const std::string& name) const;

  // Check if the adapter is enabled
  bool IsEnabled() const;

 private:
  SystemCliAdapter() = default;
  ~SystemCliAdapter() = default;
  SystemCliAdapter(const SystemCliAdapter&) = delete;
  SystemCliAdapter& operator=(const SystemCliAdapter&) = delete;

  bool LoadConfig(const std::string& config_path);
  void LoadToolDocs(const std::string& tools_dir);
  void RegisterCapabilities();

  bool enabled_ = false;
  std::string tools_dir_;
  std::map<std::string, SystemCliToolConfig> tools_;
  std::map<std::string, std::string> tool_docs_;
  mutable std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // SYSTEM_CLI_ADAPTER_HH_
