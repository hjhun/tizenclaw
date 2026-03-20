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
  std::string app_id;
  std::string label;
  std::string icon;
  std::string exec;
  std::string package;
  std::string type;
  std::string component_type;
  bool nodisplay = false;
};

std::string EscapeJson(const std::string& s) {
  std::string result;
  result.reserve(s.size());
  for (char c : s) {
    switch (c) {
      case '"':  result += "\\\""; break;
      case '\\': result += "\\\\"; break;
      case '\n': result += "\\n";  break;
      case '\r': result += "\\r";  break;
      case '\t': result += "\\t";  break;
      default:   result += c;      break;
    }
  }
  return result;
}

std::string GetStr(app_info_h info,
                   int (*fn)(app_info_h, char**)) {
  char* v = nullptr;
  if (fn(info, &v) == 0 && v) {
    std::string result(v);
    free(v);
    return result;
  }
  return "";
}

std::string ComponentTypeToStr(
    app_info_app_component_type_e comp) {
  switch (comp) {
    case APP_INFO_APP_COMPONENT_TYPE_UI_APP:
      return "ui_app";
    case APP_INFO_APP_COMPONENT_TYPE_SERVICE_APP:
      return "service_app";
    case APP_INFO_APP_COMPONENT_TYPE_WIDGET_APP:
      return "widget_app";
    case APP_INFO_APP_COMPONENT_TYPE_WATCH_APP:
      return "watch_app";
    default:
      return "unknown";
  }
}

void PopulateEntry(app_info_h info, AppEntry& entry) {
  entry.app_id = GetStr(info, app_info_get_app_id);
  entry.label = GetStr(info, app_info_get_label);
  entry.icon = GetStr(info, app_info_get_icon);
  entry.exec = GetStr(info, app_info_get_exec);
  entry.package = GetStr(info, app_info_get_package);
  entry.type = GetStr(info, app_info_get_type);

  app_info_app_component_type_e comp;
  if (app_info_get_app_component_type(info, &comp) == 0)
    entry.component_type = ComponentTypeToStr(comp);
  else
    entry.component_type = "unknown";

  bool nodisplay_val = false;
  if (app_info_is_nodisplay(info, &nodisplay_val) == 0)
    entry.nodisplay = nodisplay_val;
}

std::string EntryToJson(const AppEntry& entry) {
  return "{\"app_id\": \"" + EscapeJson(entry.app_id) +
         "\", \"label\": \"" + EscapeJson(entry.label) +
         "\", \"icon\": \"" + EscapeJson(entry.icon) +
         "\", \"exec\": \"" + EscapeJson(entry.exec) +
         "\", \"package\": \"" + EscapeJson(entry.package) +
         "\", \"type\": \"" + EscapeJson(entry.type) +
         "\", \"component_type\": \"" +
         EscapeJson(entry.component_type) +
         "\", \"nodisplay\": " +
         (entry.nodisplay ? "true" : "false") + "}";
}

bool UiAppInfoCb(app_info_h info, void* user_data) {
  auto* apps = static_cast<std::vector<AppEntry>*>(user_data);

  app_info_app_component_type_e comp;
  if (app_info_get_app_component_type(info, &comp) == 0 &&
      comp != APP_INFO_APP_COMPONENT_TYPE_UI_APP) {
    return true;
  }

  AppEntry entry;
  PopulateEntry(info, entry);
  apps->push_back(std::move(entry));
  return true;
}

bool AllAppInfoCb(app_info_h info, void* user_data) {
  auto* apps = static_cast<std::vector<AppEntry>*>(user_data);

  AppEntry entry;
  PopulateEntry(info, entry);
  apps->push_back(std::move(entry));
  return true;
}

std::string BuildJsonResult(
    const std::vector<AppEntry>& apps) {
  std::string result = "{\"apps\": [";
  for (size_t i = 0; i < apps.size(); ++i) {
    if (i > 0)
      result += ", ";
    result += EntryToJson(apps[i]);
  }
  result += "], \"count\": " +
            std::to_string(apps.size()) + "}";
  return result;
}

}  // namespace

std::string ListAppsController::ListApps() const {
  std::vector<AppEntry> apps;
  app_manager_foreach_app_info(UiAppInfoCb, &apps);
  return BuildJsonResult(apps);
}

std::string ListAppsController::ListAllApps() const {
  std::vector<AppEntry> apps;
  app_manager_foreach_app_info(AllAppInfoCb, &apps);
  return BuildJsonResult(apps);
}

}  // namespace cli
}  // namespace tizenclaw
