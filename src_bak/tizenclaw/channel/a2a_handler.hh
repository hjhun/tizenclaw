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
#ifndef A2A_HANDLER_HH
#define A2A_HANDLER_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

class AgentCore;  // forward declaration

// A2A task status lifecycle
enum class A2ATaskStatus {
  kSubmitted,
  kWorking,
  kInputRequired,
  kCompleted,
  kFailed,
  kCancelled,
};

// A2A task representation
struct A2ATask {
  std::string id;
  A2ATaskStatus status = A2ATaskStatus::kSubmitted;
  std::string session_id;
  nlohmann::json message;
  nlohmann::json artifacts;
  std::string created_at;
  std::string updated_at;
};

// A2A protocol handler
class A2AHandler {
 public:
  explicit A2AHandler(AgentCore* agent);

  // Agent Card (/.well-known/agent.json)
  nlohmann::json GetAgentCard() const;

  // JSON-RPC 2.0 method dispatch
  nlohmann::json HandleJsonRpc(const nlohmann::json& request);

  // Validate bearer token
  bool ValidateBearerToken(const std::string& token) const;

  // Load A2A config (bearer tokens etc.)
  bool LoadConfig(const std::string& config_path);

 private:
  // JSON-RPC methods
  nlohmann::json TaskSend(const nlohmann::json& params);
  nlohmann::json TaskGet(const nlohmann::json& params);
  nlohmann::json TaskCancel(const nlohmann::json& params);

  // Helpers
  std::string GenerateTaskId() const;
  std::string GetTimestamp() const;
  std::string TaskStatusToString(A2ATaskStatus status) const;

  // JSON-RPC error helpers
  static nlohmann::json JsonRpcError(int code, const std::string& message,
                                     const nlohmann::json& id);
  static nlohmann::json JsonRpcResult(const nlohmann::json& result,
                                      const nlohmann::json& id);

  AgentCore* agent_;
  std::map<std::string, A2ATask> tasks_;
  mutable std::mutex tasks_mutex_;

  // Config
  std::vector<std::string> bearer_tokens_;
  std::string agent_name_;
  std::string agent_description_;
  std::string agent_url_;
};

}  // namespace tizenclaw

#endif  // A2A_HANDLER_HH
