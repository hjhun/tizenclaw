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

#include "mcp_client_manager.hh"
#include "../../common/logging.hh"

#include <fstream>
#include <iostream>

namespace tizenclaw {

McpClientManager::McpClientManager() {}

McpClientManager::~McpClientManager() {
  std::lock_guard<std::mutex> lock(clients_mutex_);
  clients_.clear(); // Triggers destructors and terminates processes
}

bool McpClientManager::LoadConfigAndConnect(const std::string& config_path) {
  std::ifstream ifs(config_path);
  if (!ifs.is_open()) {
    return false; // Valid since users may not have any MCP servers configured
  }

  nlohmann::json config;
  try {
    ifs >> config;
  } catch (const std::exception& e) {
    LOG(ERROR) << "MCP Client Manager: Parse error in config " << config_path << ": " << e.what();
    return false;
  }

  if (!config.contains("mcpServers") || !config["mcpServers"].is_object()) {
    return false;
  }

  std::lock_guard<std::mutex> lock(clients_mutex_);

  for (auto& el : config["mcpServers"].items()) {
    std::string server_name = el.key();
    auto srv_config = el.value();

    if (!srv_config.contains("command") || !srv_config["command"].is_string()) {
      LOG(WARNING) << "MCP Client Manager: Missing 'command' for " << server_name;
      continue;
    }

    std::string command = srv_config["command"];
    std::vector<std::string> args;
    if (srv_config.contains("args") && srv_config["args"].is_array()) {
      for (auto& a : srv_config["args"]) {
        args.push_back(a.get<std::string>());
      }
    }

    // Pass environment variables to the child? The fork/execvp handles inheritance,
    // so it gets the agent's env, but we could explicitly pass them in future.

    auto client = std::make_shared<McpClient>(server_name, command, args);
    if (client->Connect()) {
      clients_[server_name] = client;
      LOG(INFO) << "MCP Client Manager: Connected to " << server_name;
    } else {
      LOG(WARNING) << "MCP Client Manager: Failed to connect to " << server_name;
    }
  }

  return true;
}

std::vector<LlmToolDecl> McpClientManager::GetToolDeclarations() {
  std::vector<LlmToolDecl> aggregated_tools;
  std::lock_guard<std::mutex> lock(clients_mutex_);

  for (auto& pair : clients_) {
    auto client = pair.second;
    if (!client->IsConnected()) continue;

    auto mcp_tools = client->GetTools();
    for (const auto& t : mcp_tools) {
      LlmToolDecl decl;
      decl.name = "mcp__" + pair.first + "__" + t.name;
      decl.description = "[MCP: " + pair.first + "] " + t.description;
      decl.parameters = t.input_schema;
      aggregated_tools.push_back(decl);
    }
  }

  return aggregated_tools;
}

bool McpClientManager::IsMcpTool(const std::string& full_tool_name) {
  return full_tool_name.rfind("mcp__", 0) == 0;
}

bool McpClientManager::ParseToolName(const std::string& full_tool_name,
                                     std::string& out_server_name,
                                     std::string& out_actual_tool_name) {
  if (!IsMcpTool(full_tool_name)) return false;

  size_t first_delim = sizeof("mcp__") - 1; // 5
  size_t second_delim = full_tool_name.find("__", first_delim);
  
  if (second_delim == std::string::npos) return false;

  out_server_name = full_tool_name.substr(first_delim, second_delim - first_delim);
  out_actual_tool_name = full_tool_name.substr(second_delim + 2);

  return true;
}

std::string McpClientManager::ExecuteTool(const std::string& full_tool_name,
                                          const nlohmann::json& args) {
  std::string server_name;
  std::string tool_name;
  
  if (!ParseToolName(full_tool_name, server_name, tool_name)) {
    return "{\"error\": \"Invalid formatting for MCP tool name\"}";
  }

  std::shared_ptr<McpClient> client;
  {
    std::lock_guard<std::mutex> lock(clients_mutex_);
    auto it = clients_.find(server_name);
    if (it == clients_.end() || !it->second->IsConnected()) {
      return "{\"error\": \"MCP Server " + server_name + " is not connected\"}";
    }
    client = it->second;
  }

  nlohmann::json result = client->CallTool(tool_name, args);
  return result.dump();
}

}  // namespace tizenclaw
