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
#include "app_lifecycle_adapter.hh"

#include <aul_app_lifecycle.h>

#include <string>

#include "../../common/logging.hh"
#include "event_bus.hh"

namespace {

const char* StateToString(
    aul_app_lifecycle_state_e state) {
  switch (state) {
    case AUL_APP_LIFECYCLE_STATE_INITIALIZED:
      return "initialized";
    case AUL_APP_LIFECYCLE_STATE_CREATED:
      return "created";
    case AUL_APP_LIFECYCLE_STATE_RESUMED:
      return "resumed";
    case AUL_APP_LIFECYCLE_STATE_PAUSED:
      return "paused";
    case AUL_APP_LIFECYCLE_STATE_DESTROYED:
      return "destroyed";
    default:
      return "unknown";
  }
}

}  // namespace

namespace tizenclaw {

AppLifecycleAdapter::~AppLifecycleAdapter() {
  Stop();
}

void AppLifecycleAdapter::Start() {
  if (started_) return;

  LOG(DEBUG) << "AppLifecycleAdapter: "
             << "registering state_changed_cb";

  int ret =
      aul_app_lifecycle_register_state_changed_cb(
          [](const char* app_id,
             pid_t pid,
             aul_app_lifecycle_state_e state,
             bool has_focus,
             void* user_data) {
            OnStateChanged(
                app_id, pid,
                static_cast<int>(state),
                has_focus, user_data);
          },
          this);

  if (ret != 0) {
    LOG(ERROR) << "AppLifecycleAdapter: "
               << "register_state_changed_cb "
               << "failed=" << ret;
    return;
  }

  started_ = true;
  LOG(INFO) << "AppLifecycleAdapter: started";
}

void AppLifecycleAdapter::Stop() {
  if (!started_) return;

  aul_app_lifecycle_deregister_state_changed_cb();
  started_ = false;
  LOG(INFO) << "AppLifecycleAdapter: stopped";
}

std::string AppLifecycleAdapter::GetName() const {
  return "AppLifecycleAdapter";
}

void AppLifecycleAdapter::OnStateChanged(
    const char* app_id,
    pid_t pid,
    int state,
    bool has_focus,
    void* user_data) {
  if (!app_id) return;

  auto aul_state =
      static_cast<aul_app_lifecycle_state_e>(state);

  LOG(DEBUG) << "AppLifecycleAdapter: "
             << "state_changed app_id=" << app_id
             << ", pid=" << pid
             << ", state=" << StateToString(aul_state)
             << ", has_focus="
             << (has_focus ? "true" : "false");

  SystemEvent ev;
  ev.type = EventType::kAppLifecycle;
  ev.source = "aul_lifecycle";
  ev.plugin_id = "builtin";
  ev.name = std::string("app.") +
            StateToString(aul_state);
  ev.data["app_id"] = app_id;
  ev.data["pid"] = pid;
  ev.data["state"] = StateToString(aul_state);
  ev.data["has_focus"] = has_focus;

  EventBus::GetInstance().Publish(std::move(ev));
}

}  // namespace tizenclaw
