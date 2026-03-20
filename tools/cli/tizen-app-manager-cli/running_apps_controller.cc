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

#include "running_apps_controller.hh"

#include <app_manager.h>

#include <cstdlib>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

struct RunningAppEntry {
  std::string app_id;
  std::string label;
  std::string icon;
  std::string exec;
  std::string package;
  std::string type;
  std::string component_type;
  bool nodisplay = false;
  int pid = -1;
  std::string state;
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

std::string GetInfoStr(app_info_h info,
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

std::string AppStateToStr(app_state_e st) {
  switch (st) {
    case APP_STATE_FOREGROUND:
      return "foreground";
    case APP_STATE_BACKGROUND:
      return "background";
    case APP_STATE_SERVICE:
      return "service";
    case APP_STATE_TERMINATED:
      return "terminated";
    default:
      return "undefined";
  }
}

void PopulateFromAppInfo(app_info_h info,
                         RunningAppEntry& entry) {
  entry.label = GetInfoStr(info, app_info_get_label);
  entry.icon = GetInfoStr(info, app_info_get_icon);
  entry.exec = GetInfoStr(info, app_info_get_exec);
  entry.package = GetInfoStr(info, app_info_get_package);
  entry.type = GetInfoStr(info, app_info_get_type);

  app_info_app_component_type_e comp;
  if (app_info_get_app_component_type(info, &comp) == 0)
    entry.component_type = ComponentTypeToStr(comp);
  else
    entry.component_type = "unknown";

  bool nodisplay_val = false;
  if (app_info_is_nodisplay(info, &nodisplay_val) == 0)
    entry.nodisplay = nodisplay_val;
}

std::string RunningEntryToJson(
    const RunningAppEntry& entry) {
  return "{\"app_id\": \"" + EscapeJson(entry.app_id) +
         "\", \"label\": \"" + EscapeJson(entry.label) +
         "\", \"pid\": " + std::to_string(entry.pid) +
         ", \"state\": \"" + EscapeJson(entry.state) +
         "\", \"icon\": \"" + EscapeJson(entry.icon) +
         "\", \"exec\": \"" + EscapeJson(entry.exec) +
         "\", \"package\": \"" +
         EscapeJson(entry.package) +
         "\", \"type\": \"" + EscapeJson(entry.type) +
         "\", \"component_type\": \"" +
         EscapeJson(entry.component_type) +
         "\", \"nodisplay\": " +
         (entry.nodisplay ? "true" : "false") + "}";
}

struct RunningCbData {
  std::vector<RunningAppEntry>* apps;
  bool ui_only;
};

bool RunningAppContextCb(app_context_h ctx,
                         void* user_data) {
  auto* data = static_cast<RunningCbData*>(user_data);

  char* app_id_raw = nullptr;
  if (app_context_get_app_id(ctx, &app_id_raw) != 0)
    return true;

  std::string app_id = app_id_raw ? app_id_raw : "";
  free(app_id_raw);

  if (app_id.empty())
    return true;

  // Get app_info to check component type and populate
  app_info_h info = nullptr;
  if (app_manager_get_app_info(app_id.c_str(),
                               &info) != 0) {
    return true;
  }

  if (data->ui_only) {
    app_info_app_component_type_e comp;
    if (app_info_get_app_component_type(info, &comp) == 0 &&
        comp != APP_INFO_APP_COMPONENT_TYPE_UI_APP) {
      app_info_destroy(info);
      return true;
    }
  }

  RunningAppEntry entry;
  entry.app_id = app_id;
  PopulateFromAppInfo(info, entry);
  app_info_destroy(info);

  // Get PID
  pid_t pid_val = -1;
  if (app_context_get_pid(ctx, &pid_val) == 0)
    entry.pid = static_cast<int>(pid_val);

  // Get app state
  app_state_e state_val = APP_STATE_UNDEFINED;
  if (app_context_get_app_state(ctx, &state_val) == 0)
    entry.state = AppStateToStr(state_val);
  else
    entry.state = "undefined";

  data->apps->push_back(std::move(entry));
  return true;
}

std::string BuildJsonResult(
    const std::vector<RunningAppEntry>& apps) {
  std::string result = "{\"apps\": [";
  for (size_t i = 0; i < apps.size(); ++i) {
    if (i > 0)
      result += ", ";
    result += RunningEntryToJson(apps[i]);
  }
  result += "], \"count\": " +
            std::to_string(apps.size()) + "}";
  return result;
}

}  // namespace

std::string RunningAppsController::ListRunningApps()
    const {
  std::vector<RunningAppEntry> apps;
  RunningCbData data{&apps, true};
  app_manager_foreach_app_context(
      RunningAppContextCb, &data);
  return BuildJsonResult(apps);
}

std::string RunningAppsController::ListAllRunningApps()
    const {
  std::vector<RunningAppEntry> apps;
  RunningCbData data{&apps, false};
  app_manager_foreach_app_context(
      RunningAppContextCb, &data);
  return BuildJsonResult(apps);
}

}  // namespace cli
}  // namespace tizenclaw
