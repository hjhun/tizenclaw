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
#ifndef ACTION_BRIDGE_HH
#define ACTION_BRIDGE_HH

#include <action.h>
#include <tizen_core.h>
#include <tizen_core_channel.h>

#include <atomic>
#include <functional>
#include <future>
#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

constexpr const char* kActionsDir = "/opt/usr/share/tizenclaw/tools/actions";

// Command types sent via channel
enum class ActionCommand : int { kList = 1, kExecute = 2, kSync = 3 };

// Callback invoked when action schemas change
// (install/uninstall/update).
using ActionChangeCallback = std::function<void()>;

// ActionBridge runs Tizen Action Framework
// C API on a dedicated tizen-core task
// (sub-thread with its own GMainLoop).
// Inter-thread communication uses
// tizen_core_channel sender/receiver pairs.
//
// Also manages per-action MD schema files
// under kActionsDir and subscribes to
// action_event_cb for live updates.
class ActionBridge {
 public:
  ActionBridge();
  ~ActionBridge();

  ActionBridge(const ActionBridge&) = delete;
  ActionBridge& operator=(const ActionBridge&) = delete;

  [[nodiscard]] bool Start();
  void Stop();

  // Blocks caller until result arrives.
  [[nodiscard]] std::string ListActions();
  [[nodiscard]] std::string ExecuteAction(const std::string& name,
                                          const nlohmann::json& arguments);

  // Full sync: fetch all actions, refresh MD
  // files, remove stale ones.
  void SyncActionSchemas();

  // Load cached action info from MD files.
  [[nodiscard]] std::vector<nlohmann::json> LoadCachedActions() const;

  // Set callback for schema changes.
  void SetChangeCallback(ActionChangeCallback cb);

 private:
  // Send a request to worker, block on result
  [[nodiscard]] std::string SendRequest(ActionCommand cmd,
                                        const std::string& payload);

  // Worker-side: channel receive callback
  static void OnRequestReceived(tizen_core_channel_object_h object,
                                void* user_data);

  // Worker-side: handle incoming request
  void HandleRequest(int id, ActionCommand cmd, const std::string& payload);

  // Worker-side: sync implementation
  void DoSync(int id);

  // Worker-side: action_foreach callback
  static bool OnActionFound(const action_h action, void* user_data);

  // Worker-side: execute result callback
  static void OnActionResult(int execution_id, const char* json_result,
                             void* user_data);

  // Worker-side: action event callback
  static void OnActionEvent(const char* action_name,
                            action_event_type_e event_type, void* user_data);

  // Send response (resolve caller promise)
  void SendResponse(int id, const std::string& result);

  // MD file helpers
  static void WriteActionMd(const std::string& name,
                            const nlohmann::json& schema);
  static void RemoveActionMd(const std::string& name);
  static void EnsureActionsDir();

  // tizen-core task (worker thread)
  tizen_core_task_h task_ = nullptr;
  tizen_core_h core_ = nullptr;

  // Channels: request (caller→worker)
  tizen_core_channel_sender_h req_sender_ = nullptr;
  tizen_core_channel_receiver_h req_receiver_ = nullptr;
  tizen_core_source_h req_source_ = nullptr;

  // Channels: response (worker→caller)
  tizen_core_channel_sender_h resp_sender_ = nullptr;
  tizen_core_channel_receiver_h resp_receiver_ = nullptr;

  // Action client handle (worker thread only)
  action_client_h client_ = nullptr;
  action_event_handler_h event_handler_ = nullptr;

  // Pending request promises
  std::mutex pending_mutex_;
  std::map<int, std::promise<std::string>> pending_;
  std::atomic<int> next_id_{1};

  // Map execution_id → request_id
  std::mutex exec_map_mutex_;
  std::map<int, int> exec_to_req_;
  std::atomic<int> next_exec_id_{1};

  // Change callback
  std::mutex cb_mutex_;
  ActionChangeCallback change_cb_;

  bool started_ = false;
};

}  // namespace tizenclaw



#endif  // ACTION_BRIDGE_HH
