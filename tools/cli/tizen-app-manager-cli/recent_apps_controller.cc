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

#include "recent_apps_controller.hh"

#include <rua_info.h>
#include <rua_manager.h>

#include <cstdlib>
#include <ctime>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

struct RecentAppEntry {
  std::string app_id;
  std::string label;
  std::string icon;
  std::string app_path;
  std::string instance_id;
  std::string instance_name;
  std::string component_id;
  std::string uri;
  std::string image;
  time_t launch_time = 0;
  bool managed_by_task_manager = false;
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

std::string GetRuaStr(rua_info_h info,
                      int (*fn)(rua_info_h, char**)) {
  char* v = nullptr;
  if (fn(info, &v) == 0 && v) {
    std::string result(v);
    free(v);
    return result;
  }
  return "";
}

std::string FormatTime(time_t t) {
  if (t == 0)
    return "";

  struct tm tm_buf;
  localtime_r(&t, &tm_buf);
  char buf[64];
  strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M:%S",
           &tm_buf);
  return std::string(buf);
}

std::string EntryToJson(const RecentAppEntry& entry) {
  return "{\"app_id\": \"" +
         EscapeJson(entry.app_id) +
         "\", \"label\": \"" +
         EscapeJson(entry.label) +
         "\", \"icon\": \"" +
         EscapeJson(entry.icon) +
         "\", \"app_path\": \"" +
         EscapeJson(entry.app_path) +
         "\", \"launch_time\": " +
         std::to_string(entry.launch_time) +
         ", \"launch_time_str\": \"" +
         EscapeJson(FormatTime(entry.launch_time)) +
         "\", \"instance_id\": \"" +
         EscapeJson(entry.instance_id) +
         "\", \"instance_name\": \"" +
         EscapeJson(entry.instance_name) +
         "\", \"component_id\": \"" +
         EscapeJson(entry.component_id) +
         "\", \"uri\": \"" +
         EscapeJson(entry.uri) +
         "\", \"image\": \"" +
         EscapeJson(entry.image) +
         "\", \"managed_by_task_manager\": " +
         (entry.managed_by_task_manager
              ? "true"
              : "false") +
         "}";
}

bool RuaInfoCb(rua_info_h info, void* user_data) {
  auto* apps =
      static_cast<std::vector<RecentAppEntry>*>(
          user_data);

  RecentAppEntry entry;
  entry.app_id =
      GetRuaStr(info, rua_info_get_app_id);
  entry.label =
      GetRuaStr(info, rua_info_get_label);
  entry.icon =
      GetRuaStr(info, rua_info_get_icon);
  entry.app_path =
      GetRuaStr(info, rua_info_get_app_path);
  entry.instance_id =
      GetRuaStr(info, rua_info_get_instance_id);
  entry.instance_name =
      GetRuaStr(info, rua_info_get_instance_name);
  entry.component_id =
      GetRuaStr(info, rua_info_get_component_id);
  entry.uri =
      GetRuaStr(info, rua_info_get_uri);
  entry.image =
      GetRuaStr(info, rua_info_get_image);

  time_t launch_time_val = 0;
  if (rua_info_get_launch_time(info,
                               &launch_time_val) == 0) {
    entry.launch_time = launch_time_val;
  }

  bool managed_val = false;
  if (rua_info_is_managed_by_task_manager(
          info, &managed_val) == 0) {
    entry.managed_by_task_manager = managed_val;
  }

  apps->push_back(std::move(entry));
  return true;
}

struct DetailCbData {
  std::string target_app_id;
  std::string app_id;
  std::string args;
  std::string uri;
  std::string instance_id;
  std::string component_id;
  time_t launch_time = 0;
  bool found = false;
};

bool DetailRuaInfoCb(rua_info_h info,
                     void* user_data) {
  auto* data = static_cast<DetailCbData*>(user_data);

  std::string aid =
      GetRuaStr(info, rua_info_get_app_id);
  if (aid != data->target_app_id)
    return true;

  data->app_id = aid;
  data->args =
      GetRuaStr(info, rua_info_get_args);
  data->uri =
      GetRuaStr(info, rua_info_get_uri);
  data->instance_id =
      GetRuaStr(info, rua_info_get_instance_id);
  data->component_id =
      GetRuaStr(info, rua_info_get_component_id);

  time_t lt = 0;
  if (rua_info_get_launch_time(info, &lt) == 0)
    data->launch_time = lt;

  data->found = true;
  return false;  // stop iteration
}

}  // namespace

std::string RecentAppsController::ListRecentApps()
    const {
  std::vector<RecentAppEntry> apps;
  rua_manager_foreach_rua_info(RuaInfoCb, &apps);

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

std::string RecentAppsController::GetRecentAppDetail(
    const std::string& app_id) const {
  DetailCbData data;
  data.target_app_id = app_id;
  rua_manager_foreach_rua_info(DetailRuaInfoCb, &data);

  if (!data.found)
    return "{\"error\": \"App not found in RUA history\"}";

  return "{\"app_id\": \"" +
         EscapeJson(data.app_id) +
         "\", \"args\": \"" +
         EscapeJson(data.args) +
         "\", \"uri\": \"" +
         EscapeJson(data.uri) +
         "\", \"instance_id\": \"" +
         EscapeJson(data.instance_id) +
         "\", \"component_id\": \"" +
         EscapeJson(data.component_id) +
         "\", \"launch_time\": " +
         std::to_string(data.launch_time) +
         ", \"launch_time_str\": \"" +
         EscapeJson(FormatTime(data.launch_time)) +
         "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
