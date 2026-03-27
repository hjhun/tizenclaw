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
#include "recent_app_adapter.hh"

#include <rua.h>
#include <rua_info.h>
#include <rua_manager.h>

#include <json.hpp>
#include <string>

#include "../../common/logging.hh"
#include "event_bus.hh"

namespace {

constexpr int kMaxRecentApps = 10;

}  // namespace

namespace tizenclaw {

RecentAppAdapter::~RecentAppAdapter() {
  Stop();
}

void RecentAppAdapter::Start() {
  if (started_) return;

  LOG(DEBUG) << "RecentAppAdapter: "
             << "initializing rua";

  int ret = rua_init();
  if (ret != 0) {
    LOG(WARNING) << "RecentAppAdapter: "
                 << "rua_init failed=" << ret;
    // Non-fatal: continue without update callback
  }

  ret = rua_register_update_cb(
      OnHistoryUpdated, this, &callback_id_);
  if (ret != 0) {
    LOG(WARNING) << "RecentAppAdapter: "
                 << "register_update_cb failed="
                 << ret;
    callback_id_ = -1;
  }

  // Publish initial snapshot
  PublishRecentApps();

  started_ = true;
  LOG(INFO) << "RecentAppAdapter: started";
}

void RecentAppAdapter::Stop() {
  if (!started_) return;

  if (callback_id_ >= 0) {
    rua_unregister_update_cb(callback_id_);
    callback_id_ = -1;
  }

  rua_fini();
  started_ = false;
  LOG(INFO) << "RecentAppAdapter: stopped";
}

std::string RecentAppAdapter::GetName() const {
  return "RecentAppAdapter";
}

void RecentAppAdapter::OnHistoryUpdated(
    char** table, int nrows,
    int ncols, void* user_data) {
  auto* self =
      static_cast<RecentAppAdapter*>(user_data);
  if (!self) return;

  LOG(DEBUG) << "RecentAppAdapter: "
             << "history updated, nrows=" << nrows
             << ", ncols=" << ncols;

  self->PublishRecentApps();
}

void RecentAppAdapter::PublishRecentApps() {
  auto apps = nlohmann::json::array();
  int count = 0;

  auto ctx = std::make_pair(&apps, &count);
  int ret = rua_manager_foreach_rua_info(
      [](rua_info_h info,
         void* user_data) -> bool {
        auto* pair = static_cast<
            std::pair<nlohmann::json*, int*>*>(
            user_data);
        auto* app_list = pair->first;
        auto* cnt = pair->second;

        if (*cnt >= kMaxRecentApps) return false;

        nlohmann::json entry;

        char* app_id = nullptr;
        if (rua_info_get_app_id(info, &app_id) == 0
            && app_id) {
          entry["app_id"] = app_id;
          free(app_id);
        }

        char* label = nullptr;
        if (rua_info_get_label(info, &label) == 0
            && label) {
          entry["label"] = label;
          free(label);
        }

        time_t launch_time = 0;
        if (rua_info_get_launch_time(
                info, &launch_time) == 0) {
          entry["launch_time"] =
              static_cast<int64_t>(launch_time);
        }

        char* comp_id = nullptr;
        if (rua_info_get_component_id(
                info, &comp_id) == 0
            && comp_id) {
          entry["component_id"] = comp_id;
          free(comp_id);
        }

        app_list->push_back(std::move(entry));
        (*cnt)++;
        return true;
      },
      &ctx);

  if (ret != 0) {
    LOG(WARNING) << "RecentAppAdapter: "
                 << "foreach_rua_info failed="
                 << ret;
    return;
  }

  LOG(DEBUG) << "RecentAppAdapter: "
             << "publishing recent_apps, count="
             << count;

  SystemEvent ev;
  ev.type = EventType::kRecentApp;
  ev.source = "rua";
  ev.plugin_id = "builtin";
  ev.name = "recent_apps.updated";
  ev.data["recent_apps"] = std::move(apps);
  ev.data["count"] = count;

  EventBus::GetInstance().Publish(std::move(ev));
}

}  // namespace tizenclaw
