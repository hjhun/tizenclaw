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

#include "action_bridge.hh"

#include <chrono>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <vector>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

constexpr int kResponseTimeoutMs = 30000;
constexpr const char* kWorkerName = "action_worker";

}  // namespace

ActionBridge::ActionBridge() = default;

ActionBridge::~ActionBridge() { Stop(); }

bool ActionBridge::Start() {
  if (started_) return true;

  LOG(INFO) << "ActionBridge: Starting worker thread";

  // Create tizen-core task (sub-thread)
  int ret = tizen_core_task_create(kWorkerName, true, &task_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to create task: " << ret;
    return false;
  }

  ret = tizen_core_task_get_tizen_core(task_, &core_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to get core: " << ret;
    tizen_core_task_destroy(task_);
    task_ = nullptr;
    return false;
  }

  // Create request channel (Caller → Worker)
  ret = tizen_core_channel_make_pair(&req_sender_, &req_receiver_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to create "
               << "request channel: " << ret;
    tizen_core_task_destroy(task_);
    task_ = nullptr;
    return false;
  }

  // Create response channel (Worker → Caller)
  ret = tizen_core_channel_make_pair(&resp_sender_, &resp_receiver_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to create "
               << "response channel: " << ret;
    tizen_core_channel_sender_destroy(req_sender_);
    tizen_core_channel_receiver_destroy(req_receiver_);
    req_sender_ = nullptr;
    req_receiver_ = nullptr;
    tizen_core_task_destroy(task_);
    task_ = nullptr;
    return false;
  }

  // Register request receiver on worker core
  ret = tizen_core_add_channel(core_, req_receiver_, OnRequestReceived, this,
                               &req_source_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to add channel: " << ret;
    Stop();
    return false;
  }

  // Start worker GMainLoop (non-blocking,
  // runs in its own thread).
  ret = tizen_core_task_run(task_);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to run task: " << ret;
    Stop();
    return false;
  }

  started_ = true;
  LOG(INFO) << "ActionBridge: Worker started";
  return true;
}

void ActionBridge::Stop() {
  if (!started_ && !task_) return;

  LOG(INFO) << "ActionBridge: Stopping worker";

  if (task_) {
    tizen_core_task_quit(task_);
  }

  // Clean up channels
  if (req_source_ && core_) {
    tizen_core_remove_source(core_, req_source_);
    req_source_ = nullptr;
  }
  if (req_sender_) {
    tizen_core_channel_sender_destroy(req_sender_);
    req_sender_ = nullptr;
  }
  if (req_receiver_) {
    tizen_core_channel_receiver_destroy(req_receiver_);
    req_receiver_ = nullptr;
  }
  if (resp_sender_) {
    tizen_core_channel_sender_destroy(resp_sender_);
    resp_sender_ = nullptr;
  }
  if (resp_receiver_) {
    tizen_core_channel_receiver_destroy(resp_receiver_);
    resp_receiver_ = nullptr;
  }

  if (task_) {
    tizen_core_task_destroy(task_);
    task_ = nullptr;
  }

  core_ = nullptr;
  client_ = nullptr;
  event_handler_ = nullptr;
  started_ = false;

  // Fail any pending requests
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    for (auto& [id, p] : pending_) {
      p.set_value("{\"error\":\"ActionBridge stopped\"}");
    }
    pending_.clear();
  }

  LOG(INFO) << "ActionBridge: Stopped";
}

std::string ActionBridge::ListActions() {
  return SendRequest(ActionCommand::kList, "{}");
}

std::string ActionBridge::ExecuteAction(const std::string& name,
                                        const nlohmann::json& arguments) {
  nlohmann::json payload = {{"name", name}, {"arguments", arguments}};
  return SendRequest(ActionCommand::kExecute, payload.dump());
}

void ActionBridge::SyncActionSchemas() {
  LOG(INFO) << "ActionBridge: Syncing schemas";
  auto result = SendRequest(ActionCommand::kSync, "{}");
  LOG(INFO) << "ActionBridge: Sync result: " << result;
}

void ActionBridge::SetChangeCallback(ActionChangeCallback cb) {
  std::lock_guard<std::mutex> lock(cb_mutex_);
  change_cb_ = std::move(cb);
}

// ---- MD file helpers ----

void ActionBridge::EnsureActionsDir() {
  namespace fs = std::filesystem;
  std::error_code ec;
  fs::create_directories(kActionsDir, ec);
  if (ec) {
    LOG(WARNING) << "ActionBridge: Failed to create " << kActionsDir << ": "
                 << ec.message();
  }
}

void ActionBridge::WriteActionMd(const std::string& name,
                                 const nlohmann::json& schema) {
  EnsureActionsDir();

  std::string path = std::string(kActionsDir) + "/" + name + ".md";

  // Extract fields from schema
  std::string desc = schema.value("description", "");
  std::string category = schema.value("category", "");

  std::ostringstream md;
  md << "# " << name << "\n\n";

  if (!desc.empty()) {
    md << desc << "\n\n";
  }
  if (!category.empty()) {
    md << "**Category**: " << category << "\n\n";
  }

  // Input parameters table
  if (schema.contains("inputSchema") &&
      schema["inputSchema"].contains("properties")) {
    const auto& props = schema["inputSchema"]["properties"];
    std::vector<std::string> required_params;
    if (schema["inputSchema"].contains("required") &&
        schema["inputSchema"]["required"].is_array()) {
      for (const auto& r : schema["inputSchema"]["required"]) {
        required_params.push_back(r.get<std::string>());
      }
    }

    md << "## Parameters\n\n";
    md << "| Name | Type | Required " << "| Description |\n";
    md << "|------|------|----------" << "|-------------|\n";

    for (auto it = props.begin(); it != props.end(); ++it) {
      std::string pname = it.key();
      std::string ptype = it.value().value("type", "string");
      std::string pdesc = it.value().value("description", "");

      bool is_required = false;
      for (const auto& r : required_params) {
        if (r == pname) {
          is_required = true;
          break;
        }
      }

      md << "| " << pname << " | " << ptype << " | "
         << (is_required ? "yes" : "no") << " | " << pdesc << " |\n";
    }
    md << "\n";
  }

  // Privileges
  if (schema.contains("requiredPrivileges") &&
      schema["requiredPrivileges"].is_array() &&
      !schema["requiredPrivileges"].empty()) {
    md << "## Privileges\n\n";
    for (const auto& priv : schema["requiredPrivileges"]) {
      md << "- " << priv.get<std::string>() << "\n";
    }
    md << "\n";
  }

  // Raw JSON schema
  md << "## Schema\n\n";
  md << "```json\n";
  md << schema.dump(2) << "\n";
  md << "```\n";

  std::ofstream out(path);
  if (!out.is_open()) {
    LOG(ERROR) << "ActionBridge: Failed to write " << path;
    return;
  }
  out << md.str();
  out.close();

  LOG(INFO) << "ActionBridge: Wrote " << path;
}

void ActionBridge::RemoveActionMd(const std::string& name) {
  std::string path = std::string(kActionsDir) + "/" + name + ".md";
  std::error_code ec;
  std::filesystem::remove(path, ec);
  if (ec) {
    LOG(WARNING) << "ActionBridge: Failed to remove " << path << ": "
                 << ec.message();
  } else {
    LOG(INFO) << "ActionBridge: Removed " << path;
  }
}

std::vector<nlohmann::json> ActionBridge::LoadCachedActions() const {
  namespace fs = std::filesystem;
  std::vector<nlohmann::json> result;

  std::error_code ec;
  if (!fs::exists(kActionsDir, ec)) return result;

  for (const auto& entry : fs::directory_iterator(kActionsDir, ec)) {
    if (!entry.is_regular_file()) continue;
    if (entry.path().extension() != ".md") continue;

    std::ifstream in(entry.path());
    if (!in.is_open()) continue;

    std::string content((std::istreambuf_iterator<char>(in)),
                        std::istreambuf_iterator<char>());

    // Extract JSON from ```json ... ``` block
    auto json_start = content.find("```json\n");
    auto json_end = content.rfind("\n```");
    if (json_start == std::string::npos || json_end == std::string::npos)
      continue;

    json_start += 8;  // skip "```json\n"
    if (json_start >= json_end) continue;

    std::string json_str = content.substr(json_start, json_end - json_start);
    try {
      auto schema = nlohmann::json::parse(json_str);
      result.push_back(std::move(schema));
    } catch (const std::exception& e) {
      LOG(WARNING) << "ActionBridge: Bad schema in " << entry.path() << ": "
                   << e.what();
    }
  }

  return result;
}

// ---- Channel communication ----

std::string ActionBridge::SendRequest(ActionCommand cmd,
                                      const std::string& payload) {
  if (!started_) {
    return "{\"error\":"
           "\"ActionBridge not started\"}";
  }

  int id = next_id_.fetch_add(1);

  // Create promise for this request
  std::future<std::string> future;
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_[id] = std::promise<std::string>();
    future = pending_[id].get_future();
  }

  // Build request data as JSON string
  nlohmann::json req_json = {
      {"id", id}, {"cmd", static_cast<int>(cmd)}, {"payload", payload}};
  std::string req_str = req_json.dump();

  // Send via channel
  tizen_core_channel_object_h obj = nullptr;
  int ret = tizen_core_channel_object_create(&obj);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_.erase(id);
    return "{\"error\":"
           "\"Failed to create channel object\"}";
  }

  tizen_core_channel_object_set_id(obj, id);
  char* data = strdup(req_str.c_str());
  tizen_core_channel_object_set_data(obj, data);

  ret = tizen_core_channel_sender_send(req_sender_, obj);
  tizen_core_channel_object_destroy(obj);

  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to send " << "request: " << ret;
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_.erase(id);
    free(data);
    return "{\"error\":"
           "\"Failed to send request\"}";
  }

  // Wait for response with timeout
  auto status = future.wait_for(std::chrono::milliseconds(kResponseTimeoutMs));
  if (status == std::future_status::timeout) {
    LOG(WARNING) << "ActionBridge: Request " << id << " timed out";
    std::lock_guard<std::mutex> lock(pending_mutex_);
    pending_.erase(id);
    return "{\"error\":\"Request timed out\"}";
  }

  return future.get();
}

void ActionBridge::OnRequestReceived(tizen_core_channel_object_h object,
                                     void* user_data) {
  auto* self = static_cast<ActionBridge*>(user_data);

  void* raw_data = nullptr;
  tizen_core_channel_object_get_data(object, &raw_data);
  if (!raw_data) return;

  auto* data = static_cast<char*>(raw_data);
  std::string req_str(data);
  free(data);

  try {
    auto req = nlohmann::json::parse(req_str);
    int id = req.value("id", 0);
    auto cmd = static_cast<ActionCommand>(req.value("cmd", 0));
    std::string payload = req.value("payload", "{}");
    self->HandleRequest(id, cmd, payload);
  } catch (const std::exception& e) {
    LOG(ERROR) << "ActionBridge: Bad request: " << e.what();
  }
}

// ---- Worker-side handlers ----

void ActionBridge::HandleRequest(int id, ActionCommand cmd,
                                 const std::string& payload) {
  // Lazy init: create action client on first
  // request (GMainLoop is now fully running)
  if (!client_) {
    LOG(INFO) << "ActionBridge: Creating action " << "client (lazy init)";
    int ret = action_client_create(&client_);
    if (ret != ACTION_ERROR_NONE) {
      LOG(ERROR) << "ActionBridge: action_client_create" << " failed: " << ret;
      client_ = nullptr;
      SendResponse(id,
                   "{\"error\":"
                   "\"Action client connect failed: " +
                       std::to_string(ret) + "\"}");
      return;
    }

    // Register event handler
    ret = action_client_add_event_handler(client_, OnActionEvent, this,
                                          &event_handler_);
    if (ret != ACTION_ERROR_NONE) {
      LOG(WARNING) << "ActionBridge: Failed to add "
                   << "event handler: " << ret;
      event_handler_ = nullptr;
    } else {
      LOG(INFO) << "ActionBridge: Event handler " << "registered";
    }

    LOG(INFO) << "ActionBridge: Action client ready";
  }

  if (cmd == ActionCommand::kSync) {
    DoSync(id);

  } else if (cmd == ActionCommand::kList) {
    // Collect all actions using foreach
    nlohmann::json actions = nlohmann::json::array();

    action_client_foreach_action(
        client_,
        [](const action_h action, void* user_data) -> bool {
          auto* arr = static_cast<nlohmann::json*>(user_data);

          char* name = nullptr;
          char* schema = nullptr;

          action_get_name(action, &name);
          action_get_schema(action, &schema);

          nlohmann::json entry;
          if (name) {
            entry["name"] = name;
            free(name);
          }
          if (schema) {
            try {
              entry["schema"] = nlohmann::json::parse(schema);
            } catch (...) {
              entry["schema"] = schema;
            }
            free(schema);
          }

          arr->push_back(entry);
          return true;
        },
        &actions);

    SendResponse(id, actions.dump());

  } else if (cmd == ActionCommand::kExecute) {
    try {
      auto req = nlohmann::json::parse(payload);
      std::string action_name = req.value("name", "");
      nlohmann::json arguments =
          req.value("arguments", nlohmann::json::object());

      if (action_name.empty()) {
        SendResponse(id,
                     "{\"error\":"
                     "\"Action name is empty\"}");
        return;
      }

      // Build JSON-RPC 2.0 model
      int exec_id = next_exec_id_.fetch_add(1);
      nlohmann::json model = {
          {"id", exec_id},
          {"params", {{"name", action_name}, {"arguments", arguments}}}};
      std::string model_str = model.dump();

      // Map execution_id → request_id
      {
        std::lock_guard<std::mutex> lock(exec_map_mutex_);
        exec_to_req_[exec_id] = id;
      }

      LOG(INFO) << "ActionBridge: Executing action '" << action_name
                << "' exec_id=" << exec_id;

      int ret = action_client_execute(client_, model_str.c_str(),
                                      OnActionResult, this);
      if (ret != ACTION_ERROR_NONE) {
        LOG(ERROR) << "ActionBridge: Execute failed: " << ret;
        {
          std::lock_guard<std::mutex> lock(exec_map_mutex_);
          exec_to_req_.erase(exec_id);
        }
        SendResponse(id,
                     "{\"error\":"
                     "\"action_client_execute failed: " +
                         std::to_string(ret) + "\"}");
      }
      // Result arrives via OnActionResult

    } catch (const std::exception& e) {
      SendResponse(id, std::string("{\"error\":\"") + e.what() + "\"}");
    }
  } else {
    SendResponse(id, "{\"error\":\"Unknown command\"}");
  }
}

void ActionBridge::DoSync(int id) {
  namespace fs = std::filesystem;
  EnsureActionsDir();

  // Collect all actions
  struct SyncData {
    std::vector<std::string> names;
    std::vector<nlohmann::json> schemas;
  };
  SyncData sync_data;

  action_client_foreach_action(
      client_,
      [](const action_h action, void* user_data) -> bool {
        auto* sd = static_cast<SyncData*>(user_data);

        char* name = nullptr;
        char* schema = nullptr;

        action_get_name(action, &name);
        action_get_schema(action, &schema);

        if (name && schema) {
          sd->names.push_back(name);
          try {
            sd->schemas.push_back(nlohmann::json::parse(schema));
          } catch (...) {
            sd->schemas.push_back(
                nlohmann::json{{"name", name}, {"raw", schema}});
          }
        }
        if (name) free(name);
        if (schema) free(schema);

        return true;
      },
      &sync_data);

  // Write MD files for all current actions
  for (size_t i = 0; i < sync_data.names.size(); ++i) {
    WriteActionMd(sync_data.names[i], sync_data.schemas[i]);
  }

  // Remove stale MD files not in current list
  std::error_code ec;
  if (fs::exists(kActionsDir, ec)) {
    for (const auto& entry : fs::directory_iterator(kActionsDir, ec)) {
      if (!entry.is_regular_file()) continue;
      if (entry.path().extension() != ".md") continue;

      std::string stem = entry.path().stem().string();
      bool found = false;
      for (const auto& n : sync_data.names) {
        if (n == stem) {
          found = true;
          break;
        }
      }
      if (!found) {
        LOG(INFO) << "ActionBridge: Removing stale " << entry.path();
        fs::remove(entry.path(), ec);
      }
    }
  }

  LOG(INFO) << "ActionBridge: Synced " << sync_data.names.size() << " actions";

  SendResponse(id,
               "{\"synced\":" + std::to_string(sync_data.names.size()) + "}");
}

// ---- Action event handling ----

void ActionBridge::OnActionEvent(const char* action_name,
                                 action_event_type_e event_type,
                                 void* user_data) {
  auto* self = static_cast<ActionBridge*>(user_data);

  std::string name = action_name ? action_name : "";

  const char* type_str = "UNKNOWN";
  switch (event_type) {
    case ACTION_EVENT_TYPE_INSTALL:
      type_str = "INSTALL";
      break;
    case ACTION_EVENT_TYPE_UNINSTALL:
      type_str = "UNINSTALL";
      break;
    case ACTION_EVENT_TYPE_UPDATE:
      type_str = "UPDATE";
      break;
  }

  LOG(INFO) << "ActionBridge: Action event: " << type_str << " for '" << name
            << "'";

  if (event_type == ACTION_EVENT_TYPE_UNINSTALL) {
    RemoveActionMd(name);
  } else {
    // INSTALL or UPDATE: fetch schema and write
    action_h action_handle = nullptr;
    int ret =
        action_client_get_action(self->client_, name.c_str(), &action_handle);
    if (ret == ACTION_ERROR_NONE && action_handle) {
      char* schema_str = nullptr;
      action_get_schema(action_handle, &schema_str);
      if (schema_str) {
        try {
          auto schema = nlohmann::json::parse(schema_str);
          WriteActionMd(name, schema);
        } catch (...) {
          LOG(WARNING) << "ActionBridge: Bad schema " << "for " << name;
        }
        free(schema_str);
      }
      action_destroy(action_handle);
    } else {
      LOG(WARNING) << "ActionBridge: Failed to get " << "action '" << name
                   << "': " << ret;
    }
  }

  // Notify AgentCore to reload tools
  {
    std::lock_guard<std::mutex> lock(self->cb_mutex_);
    if (self->change_cb_) {
      self->change_cb_();
    }
  }
}

// ---- Callbacks ----

void ActionBridge::OnActionResult(int execution_id, const char* json_result,
                                  void* user_data) {
  auto* self = static_cast<ActionBridge*>(user_data);

  LOG(INFO) << "ActionBridge: Action result for " << "exec_id=" << execution_id;

  int req_id = -1;
  {
    std::lock_guard<std::mutex> lock(self->exec_map_mutex_);
    auto it = self->exec_to_req_.find(execution_id);
    if (it != self->exec_to_req_.end()) {
      req_id = it->second;
      self->exec_to_req_.erase(it);
    }
  }

  if (req_id < 0) {
    LOG(WARNING) << "ActionBridge: No pending request "
                 << "for exec_id=" << execution_id;
    return;
  }

  std::string result =
      json_result ? json_result : "{\"error\":\"null result\"}";
  self->SendResponse(req_id, result);
}

void ActionBridge::SendResponse(int id, const std::string& result) {
  // Send via response channel
  tizen_core_channel_object_h obj = nullptr;
  int ret = tizen_core_channel_object_create(&obj);
  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to create "
               << "response channel object";
    std::lock_guard<std::mutex> lock(pending_mutex_);
    auto it = pending_.find(id);
    if (it != pending_.end()) {
      it->second.set_value(result);
      pending_.erase(it);
    }
    return;
  }

  tizen_core_channel_object_set_id(obj, id);

  nlohmann::json resp = {{"id", id}, {"result", result}};
  char* data = strdup(resp.dump().c_str());
  tizen_core_channel_object_set_data(obj, data);

  ret = tizen_core_channel_sender_send(resp_sender_, obj);
  tizen_core_channel_object_destroy(obj);

  if (ret != TIZEN_CORE_ERROR_NONE) {
    LOG(ERROR) << "ActionBridge: Failed to send " << "response via channel";
    free(data);
  }

  // Resolve promise directly
  {
    std::lock_guard<std::mutex> lock(pending_mutex_);
    auto it = pending_.find(id);
    if (it != pending_.end()) {
      it->second.set_value(result);
      pending_.erase(it);
    }
  }
}

}  // namespace tizenclaw

