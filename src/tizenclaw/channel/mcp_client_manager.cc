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

McpClientManager::McpClientManager() : is_running_(true) {
  idle_monitor_thread_ = std::thread(&McpClientManager::IdleMonitorLoop, this);
}

McpClientManager::~McpClientManager() {
  is_running_ = false;
  if (idle_monitor_thread_.joinable()) {
    idle_monitor_thread_.join();
  }
  std::lock_guard<std::mutex> lock(clients_mutex_);
  clients_.clear();
  configs_.clear();
}

void McpClientManager::IdleMonitorLoop() {
  while (is_running_) {
    std::this_thread::sleep_for(std::chrono::seconds(2));
    if (!is_running_) break;

    std::lock_guard<std::mutex> lock(clients_mutex_);
    auto now_ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                      std::chrono::system_clock::now().time_since_epoch())
                      .count();

    for (auto& pair : clients_) {
      const std::string& server_name = pair.first;
      auto client = pair.second;
      auto it_cfg = configs_.find(server_name);
      
      if (it_cfg != configs_.end() && it_cfg->second.idle_timeout_sec > 0 && client->IsConnected()) {
        long long last_used = client->GetLastUsedMs();
        if ((now_ms - last_used) > (it_cfg->second.idle_timeout_sec * 1000LL)) {
          LOG(INFO) << "MCP Client Manager: Idle timeout reached for " << server_name << ", disconnecting.";
          client->Disconnect();
        }
      }
    }
  }
}

bool McpClientManager::LoadConfigAndConnect(const std::string& config_path) {
  std::ifstream ifs(config_path);
  if (!ifs.is_open()) return false;

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

    ServerConfig srv;
    srv.command = srv_config["command"].get<std::string>();
    
    if (srv_config.contains("args") && srv_config["args"].is_array()) {
      for (auto& a : srv_config["args"]) {
        srv.args.push_back(a.get<std::string>());
      }
    }
    
    if (srv_config.contains("sandbox") && srv_config["sandbox"].is_boolean()) {
      srv.is_sandbox = srv_config["sandbox"].get<bool>();
    }
    if (srv_config.contains("timeout_seconds") && srv_config["timeout_seconds"].is_number()) {
      srv.timeout_ms = srv_config["timeout_seconds"].get<int>() * 1000;
    }
    if (srv_config.contains("idle_timeout_seconds") && srv_config["idle_timeout_seconds"].is_number()) {
      srv.idle_timeout_sec = srv_config["idle_timeout_seconds"].get<int>();
    }

    std::string actual_command = srv.command;
    std::vector<std::string> actual_args = srv.args;

    if (srv.is_sandbox) {
      actual_command = "/usr/libexec/tizenclaw/crun";
      actual_args = {"exec", "tizenclaw_code_sandbox", srv.command};
      for (const auto& a : srv.args) {
        actual_args.push_back(a);
      }
    }

    auto client = std::make_shared<McpClient>(server_name, actual_command, actual_args, srv.timeout_ms);
    if (client->Connect()) {
      // Cache tools on boot so LLM sees them even if idle-disconnected
      auto tools = client->GetTools();
      for (const auto& t : tools) {
        LlmToolDecl decl;
        decl.name = "mcp__" + server_name + "__" + t.name;
        decl.description = "[MCP: " + server_name + "] " + t.description;
        decl.parameters = t.input_schema;
        srv.cached_tools.push_back(decl);
      }
      srv.loaded = true;
      clients_[server_name] = client;
      LOG(INFO) << "MCP Client Manager: Connected and cached tools for " << server_name;
    } else {
      LOG(WARNING) << "MCP Client Manager: Failed to connect to " << server_name;
    }
    
    configs_[server_name] = srv;
  }

  return true;
}

std::vector<LlmToolDecl> McpClientManager::GetToolDeclarations() {
  std::vector<LlmToolDecl> aggregated_tools;
  std::lock_guard<std::mutex> lock(clients_mutex_);

  for (const auto& pair : configs_) {
    if (pair.second.loaded) {
      for (const auto& decl : pair.second.cached_tools) {
        aggregated_tools.push_back(decl);
      }
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
    if (it == clients_.end() || !configs_[server_name].loaded) {
      return "{\"error\": \"MCP Server " + server_name + " is not configured\"}";
    }
    client = it->second;
  }

  if (!client->IsConnected()) {
    LOG(INFO) << "MCP Client Manager: Reconnecting to idle server " << server_name;
    if (!client->Connect()) {
      return "{\"error\": \"MCP Server " + server_name + " failed to wake up from idle\"}";
    }
  }

  nlohmann::json result = client->CallTool(tool_name, args);
  return result.dump();
}

}  // namespace tizenclaw
