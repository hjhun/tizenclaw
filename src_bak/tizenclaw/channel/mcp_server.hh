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
#ifndef MCP_SERVER_HH
#define MCP_SERVER_HH

#include <json.hpp>
#include <string>
#include <vector>

#include "channel.hh"

namespace tizenclaw {

class AgentCore;

class McpServer : public Channel {
 public:
  explicit McpServer(AgentCore* agent);

  // Channel interface — MCP runs via --mcp-stdio
  // in a separate process, so Start/Stop are
  // no-ops in daemon context.
  std::string GetName() const override { return "mcp"; }
  bool Start() override { return true; }
  void Stop() override {}
  bool IsRunning() const override { return false; }

  // Run stdio JSON-RPC 2.0 loop (blocking).
  // Reads from stdin, writes to stdout.
  void RunStdio();

  // Process a single JSON-RPC 2.0 request and
  // return the response (or null json if
  // notification).
  nlohmann::json ProcessRequest(const nlohmann::json& request);

 private:
  // JSON-RPC 2.0 method handlers
  nlohmann::json HandleInitialize(const nlohmann::json& params);
  nlohmann::json HandleToolsList(const nlohmann::json& params);
  nlohmann::json HandleToolsCall(const nlohmann::json& params, int stdout_fd);

  // Discover tools from skill manifests
  void DiscoverTools();

  AgentCore* agent_;

  struct ToolInfo {
    std::string name;
    std::string description;
    nlohmann::json input_schema;
    bool is_skill = true;  // false for ask_tizenclaw
  };
  std::vector<ToolInfo> tools_;

  static constexpr const char* kVersion = "1.0.0";
  static constexpr const char* kProtocolVersion = "2024-11-05";
};

}  // namespace tizenclaw

#endif  // MCP_SERVER_HH
