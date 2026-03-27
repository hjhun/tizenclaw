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
#include "a2a_handler.hh"

#include <chrono>
#include <fstream>
#include <iomanip>
#include <random>
#include <sstream>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"
#include "../storage/audit_logger.hh"

namespace tizenclaw {

A2AHandler::A2AHandler(AgentCore* agent)
    : agent_(agent),
      agent_name_("TizenClaw Agent"),
      agent_description_(
          "TizenClaw AI Agent System for "
          "Tizen devices"),
      agent_url_("http://localhost:9090") {}

bool A2AHandler::LoadConfig(const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "A2A config not found: " << config_path;
    return false;
  }

  try {
    nlohmann::json config;
    f >> config;
    f.close();

    if (config.contains("bearer_tokens") &&
        config["bearer_tokens"].is_array()) {
      bearer_tokens_.clear();
      for (auto& t : config["bearer_tokens"]) {
        bearer_tokens_.push_back(t.get<std::string>());
      }
    }

    agent_name_ = config.value("agent_name", agent_name_);
    agent_description_ = config.value("agent_description", agent_description_);
    agent_url_ = config.value("agent_url", agent_url_);

    LOG(INFO) << "A2A config loaded: " << bearer_tokens_.size()
              << " bearer tokens";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "A2A config parse error: " << e.what();
    return false;
  }
}

bool A2AHandler::ValidateBearerToken(const std::string& token) const {
  // If no tokens configured, allow all
  // (development mode)
  if (bearer_tokens_.empty()) {
    return true;
  }

  for (auto& t : bearer_tokens_) {
    if (t == token) {
      return true;
    }
  }
  return false;
}

nlohmann::json A2AHandler::GetAgentCard() const {
  nlohmann::json card = {
      {"name", agent_name_},
      {"description", agent_description_},
      {"url", agent_url_},
      {"version", "1.0.0"},
      {"protocol", "a2a"},
      {"protocolVersion", "0.1"},
      {"capabilities",
       {{"streaming", false},
        {"pushNotifications", false},
        {"stateTransitionHistory", false}}},
      {"authentication",
       {{"schemes", nlohmann::json::array(
                        {{{"scheme", "bearer"},
                          {"description", "Bearer token authentication"}}})}}},
      {"defaultInputModes", nlohmann::json::array({"text"})},
      {"defaultOutputModes", nlohmann::json::array({"text"})},
      {"skills", nlohmann::json::array({{{"id", "general"},
                                         {"name", "General Assistant"},
                                         {"description",
                                          "General-purpose AI assistant "
                                          "for Tizen device management"}},
                                        {{"id", "device_control"},
                                         {"name", "Device Controller"},
                                         {"description",
                                          "Control and monitor Tizen "
                                          "devices"}},
                                        {{"id", "code_execution"},
                                         {"name", "Code Executor"},
                                         {"description",
                                          "Execute code in sandboxed "
                                          "containers"}}})}};

  return card;
}

std::string A2AHandler::GenerateTaskId() const {
  static std::atomic<int> counter{0};
  auto now = std::chrono::system_clock::now();
  auto ts = std::chrono::duration_cast<std::chrono::milliseconds>(
                now.time_since_epoch())
                .count();
  int seq = counter.fetch_add(1);
  std::ostringstream oss;
  oss << "a2a-" << std::hex << ts << "-" << seq;
  return oss.str();
}

std::string A2AHandler::GetTimestamp() const {
  auto now = std::chrono::system_clock::now();
  auto time = std::chrono::system_clock::to_time_t(now);
  std::ostringstream oss;
  oss << std::put_time(std::gmtime(&time), "%Y-%m-%dT%H:%M:%SZ");
  return oss.str();
}

std::string A2AHandler::TaskStatusToString(A2ATaskStatus status) const {
  switch (status) {
    case A2ATaskStatus::kSubmitted:
      return "submitted";
    case A2ATaskStatus::kWorking:
      return "working";
    case A2ATaskStatus::kInputRequired:
      return "input-required";
    case A2ATaskStatus::kCompleted:
      return "completed";
    case A2ATaskStatus::kFailed:
      return "failed";
    case A2ATaskStatus::kCancelled:
      return "cancelled";
    default:
      return "unknown";
  }
}

nlohmann::json A2AHandler::JsonRpcError(int code, const std::string& message,
                                        const nlohmann::json& id) {
  return {{"jsonrpc", "2.0"},
          {"id", id},
          {"error", {{"code", code}, {"message", message}}}};
}

nlohmann::json A2AHandler::JsonRpcResult(const nlohmann::json& result,
                                         const nlohmann::json& id) {
  return {{"jsonrpc", "2.0"}, {"id", id}, {"result", result}};
}

nlohmann::json A2AHandler::HandleJsonRpc(const nlohmann::json& request) {
  // Validate JSON-RPC 2.0 structure
  if (!request.contains("jsonrpc") || request["jsonrpc"] != "2.0") {
    return JsonRpcError(-32600, "Invalid Request",
                        request.value("id", nlohmann::json(nullptr)));
  }

  if (!request.contains("method") || !request["method"].is_string()) {
    return JsonRpcError(-32600, "Missing method",
                        request.value("id", nlohmann::json(nullptr)));
  }

  std::string method = request["method"].get<std::string>();
  nlohmann::json id = request.value("id", nlohmann::json(nullptr));
  nlohmann::json params = request.value("params", nlohmann::json::object());

  LOG(INFO) << "A2A JSON-RPC: method=" << method;

  AuditLogger::Instance().Log(AuditLogger::MakeEvent(
      AuditEventType::kToolExecution, "",
      {{"operation", "a2a_jsonrpc"}, {"method", method}}));

  // Route to method handlers
  if (method == "tasks/send") {
    return JsonRpcResult(TaskSend(params), id);
  } else if (method == "tasks/get") {
    return JsonRpcResult(TaskGet(params), id);
  } else if (method == "tasks/cancel") {
    return JsonRpcResult(TaskCancel(params), id);
  } else {
    return JsonRpcError(-32601, "Method not found: " + method, id);
  }
}

nlohmann::json A2AHandler::TaskSend(const nlohmann::json& params) {
  // Extract message
  if (!params.contains("message")) {
    return {{"error", "message is required"}};
  }

  nlohmann::json message = params["message"];
  std::string text;

  // Extract text from message parts
  if (message.contains("parts") && message["parts"].is_array()) {
    for (auto& part : message["parts"]) {
      if (part.contains("text")) {
        text += part["text"].get<std::string>();
      }
    }
  } else if (message.contains("text")) {
    text = message["text"].get<std::string>();
  }

  if (text.empty()) {
    return {{"error", "No text content in message"}};
  }

  // Create A2A task
  A2ATask task;
  task.id = GenerateTaskId();
  task.status = A2ATaskStatus::kSubmitted;
  task.message = message;
  task.created_at = GetTimestamp();
  task.updated_at = task.created_at;
  task.session_id = "a2a_" + task.id;
  task.artifacts = nlohmann::json::array();

  {
    std::lock_guard<std::mutex> lock(tasks_mutex_);
    tasks_[task.id] = task;
  }

  LOG(INFO) << "A2A task created: " << task.id;

  // Update status to working
  {
    std::lock_guard<std::mutex> lock(tasks_mutex_);
    tasks_[task.id].status = A2ATaskStatus::kWorking;
    tasks_[task.id].updated_at = GetTimestamp();
  }

  // Process via AgentCore (synchronous)
  std::string result = agent_->ProcessPrompt(task.session_id, text);

  // Update task with result
  {
    std::lock_guard<std::mutex> lock(tasks_mutex_);
    auto it = tasks_.find(task.id);
    if (it != tasks_.end()) {
      it->second.status = A2ATaskStatus::kCompleted;
      it->second.updated_at = GetTimestamp();
      it->second.artifacts =
          nlohmann::json::array({{{"type", "text"}, {"text", result}}});
    }
  }

  LOG(INFO) << "A2A task completed: " << task.id;

  // Return task object
  return {{"id", task.id},
          {"status", TaskStatusToString(A2ATaskStatus::kCompleted)},
          {"artifacts",
           nlohmann::json::array({{{"type", "text"}, {"text", result}}})},
          {"created_at", task.created_at},
          {"updated_at", GetTimestamp()}};
}

nlohmann::json A2AHandler::TaskGet(const nlohmann::json& params) {
  if (!params.contains("id") || !params["id"].is_string()) {
    return {{"error", "id is required"}};
  }

  std::string task_id = params["id"].get<std::string>();

  std::lock_guard<std::mutex> lock(tasks_mutex_);
  auto it = tasks_.find(task_id);
  if (it == tasks_.end()) {
    return {{"error", "Task not found: " + task_id}};
  }

  auto& task = it->second;
  return {{"id", task.id},
          {"status", TaskStatusToString(task.status)},
          {"artifacts", task.artifacts},
          {"created_at", task.created_at},
          {"updated_at", task.updated_at}};
}

nlohmann::json A2AHandler::TaskCancel(const nlohmann::json& params) {
  if (!params.contains("id") || !params["id"].is_string()) {
    return {{"error", "id is required"}};
  }

  std::string task_id = params["id"].get<std::string>();

  std::lock_guard<std::mutex> lock(tasks_mutex_);
  auto it = tasks_.find(task_id);
  if (it == tasks_.end()) {
    return {{"error", "Task not found: " + task_id}};
  }

  // Only allow cancellation of non-terminal
  // states
  if (it->second.status == A2ATaskStatus::kCompleted ||
      it->second.status == A2ATaskStatus::kFailed ||
      it->second.status == A2ATaskStatus::kCancelled) {
    return {{"error",
             "Cannot cancel task in terminal "
             "state: " +
                 TaskStatusToString(it->second.status)}};
  }

  it->second.status = A2ATaskStatus::kCancelled;
  it->second.updated_at = GetTimestamp();

  return {{"id", task_id},
          {"status", TaskStatusToString(A2ATaskStatus::kCancelled)},
          {"updated_at", it->second.updated_at}};
}

}  // namespace tizenclaw
