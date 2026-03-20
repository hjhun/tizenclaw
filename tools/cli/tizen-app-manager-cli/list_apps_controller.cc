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

#include "list_apps_controller.hh"

#include <app_manager.h>

#include <cstdlib>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

struct AppEntry {
  std::string id;
  std::string label;
};

bool AppInfoCb(app_info_h info, void* user_data) {
  auto* apps = static_cast<std::vector<AppEntry>*>(user_data);

  char* id = nullptr;
  char* label = nullptr;
  app_info_get_app_id(info, &id);
  app_info_get_label(info, &label);

  app_info_app_component_type_e comp;
  if (app_info_get_app_component_type(info, &comp) == 0 &&
      comp != APP_INFO_APP_COMPONENT_TYPE_UI_APP) {
    if (id)
      free(id);
    if (label)
      free(label);
    return true;
  }

  AppEntry entry;
  entry.id = id ? id : "";
  entry.label = label ? label : "";

  if (id)
    free(id);
  if (label)
    free(label);

  apps->push_back(entry);
  return true;
}

}  // namespace

std::string ListAppsController::ListApps() const {
  std::vector<AppEntry> apps;
  app_manager_foreach_app_info(AppInfoCb, &apps);

  std::string result = "{\"apps\": [";
  for (size_t i = 0; i < apps.size(); ++i) {
    if (i > 0)
      result += ", ";
    result += "{\"app_id\": \"" + apps[i].id +
              "\", \"label\": \"" + apps[i].label + "\"}";
  }
  result += "], \"count\": " +
            std::to_string(apps.size()) + "}";

  return result;
}

}  // namespace cli
}  // namespace tizenclaw
