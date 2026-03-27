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
#ifndef TOOL_DISPATCHER_HH_
#define TOOL_DISPATCHER_HH_

#include <json.hpp>
#include <functional>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

namespace tizenclaw {

// Centralized tool dispatch registry.
// Extracted from AgentCore for modular design
// and independent testability.
class ToolDispatcher {
 public:
  // Tool handler signature:
  //   args, tool_name, session_id → result string
  using ToolHandler = std::function<std::string(
      const nlohmann::json&, const std::string&,
      const std::string&)>;

  ToolDispatcher() = default;
  ~ToolDispatcher() = default;

  // Register a tool handler
  void Register(const std::string& name,
                ToolHandler handler);

  // Unregister a tool handler
  void Unregister(const std::string& name);

  // Execute a tool by name
  [[nodiscard]] std::string Execute(
      const std::string& name,
      const nlohmann::json& args,
      const std::string& session_id);

  // List all registered tool names
  [[nodiscard]] std::vector<std::string>
  ListTools() const;

  // Check if a tool is registered
  [[nodiscard]] bool HasTool(
      const std::string& name) const;

  // Get number of registered tools
  [[nodiscard]] size_t Size() const;

 private:
  std::unordered_map<std::string, ToolHandler>
      handlers_;
  mutable std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // TOOL_DISPATCHER_HH_
