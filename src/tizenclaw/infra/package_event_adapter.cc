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
#include "package_event_adapter.hh"

#include <pkgmgr-info.h>

#include <json.hpp>
#include <string>

#include "../../common/logging.hh"
#include "event_bus.hh"

namespace tizenclaw {

PackageEventAdapter::~PackageEventAdapter() {
  Stop();
}

void PackageEventAdapter::Start() {
  if (started_) return;

  LOG(DEBUG) << "PackageEventAdapter: "
             << "initializing pkgmgr client";

  client_ = pkgmgr_client_new(PC_LISTENING);
  if (!client_) {
    LOG(ERROR) << "PackageEventAdapter: "
               << "pkgmgr_client_new failed";
    return;
  }

  int ret = pkgmgr_client_set_status_type(
      client_, PKGMGR_CLIENT_STATUS_ALL);
  if (ret < 0) {
    LOG(WARNING) << "PackageEventAdapter: "
                 << "set_status_type failed="
                 << ret;
  }

  ret = pkgmgr_client_listen_status(
      client_, OnPackageEvent, this);
  if (ret < 0) {
    LOG(ERROR) << "PackageEventAdapter: "
               << "listen_status failed=" << ret;
    pkgmgr_client_free(client_);
    client_ = nullptr;
    return;
  }

  started_ = true;
  LOG(INFO) << "PackageEventAdapter: started";
}

void PackageEventAdapter::Stop() {
  if (!started_) return;

  if (client_) {
    pkgmgr_client_remove_listen_status(client_);
    pkgmgr_client_free(client_);
    client_ = nullptr;
  }

  started_ = false;
  LOG(INFO) << "PackageEventAdapter: stopped";
}

std::string PackageEventAdapter::GetName() const {
  return "PackageEventAdapter";
}

int PackageEventAdapter::OnPackageEvent(
    uid_t /*target_uid*/, int /*req_id*/,
    const char* pkg_type,
    const char* pkg_name,
    const char* key, const char* val,
    const void* /*pmsg*/, void* /*user_data*/) {
  if (!pkg_name || !key) return 0;

  LOG(DEBUG) << "PackageEventAdapter: "
             << "raw event pkg=" << pkg_name
             << ", type="
             << (pkg_type ? pkg_type : "null")
             << ", key=" << key
             << ", val="
             << (val ? val : "null");

  // Only publish on "end" (completed) or "error"
  // (failed) to avoid flooding with progress events
  bool is_end = (strcasecmp(key, "end") == 0);
  bool is_error = (strcasecmp(key, "error") == 0);
  if (!is_end && !is_error) return 0;

  SystemEvent ev;
  ev.type = EventType::kPackageChanged;
  ev.source = "package_manager";
  ev.plugin_id = "builtin";

  // val on "start" contains the event type string
  // (install/uninstall/update/move/clear).
  // On "end", val is "ok" or an error string.
  // We keep the event name generic here.
  ev.name = "package.event";

  ev.data["package_id"] = pkg_name;
  ev.data["package_type"] =
      pkg_type ? pkg_type : "unknown";
  ev.data["key"] = key;
  ev.data["value"] = val ? val : "";

  if (is_end) {
    if (val && strcasecmp(val, "ok") == 0) {
      ev.data["state"] = "completed";
      ev.data["apps"] = QueryAppInfo(pkg_name);
      LOG(DEBUG) << "PackageEventAdapter: "
                 << "completed, pkg=" << pkg_name;
    } else {
      ev.data["state"] = "failed";
      LOG(DEBUG) << "PackageEventAdapter: "
                 << "failed, pkg=" << pkg_name
                 << ", val="
                 << (val ? val : "null");
    }
  } else {
    ev.data["state"] = "failed";
    LOG(DEBUG) << "PackageEventAdapter: "
               << "error, pkg=" << pkg_name;
  }

  EventBus::GetInstance().Publish(std::move(ev));
  return 0;
}

nlohmann::json PackageEventAdapter::QueryAppInfo(
    const char* package_id) {
  auto apps = nlohmann::json::array();

  pkgmgrinfo_pkginfo_h pkg_info = nullptr;
  int ret = pkgmgrinfo_pkginfo_get_pkginfo(
      package_id, &pkg_info);
  if (ret != PMINFO_R_OK || !pkg_info)
    return apps;

  pkgmgrinfo_appinfo_get_list(
      pkg_info, PMINFO_ALL_APP,
      [](pkgmgrinfo_appinfo_h info,
         void* user_data) -> int {
        auto* app_list =
            static_cast<nlohmann::json*>(user_data);

        char* app_id = nullptr;
        if (pkgmgrinfo_appinfo_get_appid(
                info, &app_id) != PMINFO_R_OK ||
            !app_id)
          return 0;

        nlohmann::json app_entry;
        app_entry["app_id"] = app_id;

        char* label = nullptr;
        if (pkgmgrinfo_appinfo_get_label(
                info, &label) == PMINFO_R_OK &&
            label) {
          app_entry["label"] = label;
        }

        char* type = nullptr;
        if (pkgmgrinfo_appinfo_get_apptype(
                info, &type) == PMINFO_R_OK &&
            type) {
          app_entry["type"] = type;
        }

        app_list->push_back(std::move(app_entry));
        return 0;
      },
      &apps);

  pkgmgrinfo_pkginfo_destroy_pkginfo(pkg_info);
  return apps;
}

}  // namespace tizenclaw
