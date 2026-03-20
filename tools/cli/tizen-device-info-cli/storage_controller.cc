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

#include "storage_controller.hh"

#include <storage.h>

#include <cstdio>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

struct StorageEntry {
  int id;
  std::string type;
  std::string state;
  std::string path;
};

constexpr const char* kTypeNames[] = {
    "internal", "external", "extended_internal"};

constexpr const char* kStateNames[] = {
    "unmountable", "removed",
    "mounted", "mounted_read_only"};

std::vector<StorageEntry> g_storages;

bool StorageCb(int storage_id,
               storage_type_e type,
               storage_state_e state,
               const char* path,
               [[maybe_unused]] void* user_data) {
  StorageEntry entry;
  entry.id = storage_id;
  entry.type =
      (type <= 2) ? kTypeNames[type] : "unknown";
  entry.state =
      (state <= 3) ? kStateNames[state] : "unknown";
  entry.path = path ? path : "";
  g_storages.push_back(entry);
  return true;
}

}  // namespace

std::string StorageController::GetStorageInfo() const {
  g_storages.clear();
  storage_foreach_device_supported(
      StorageCb, nullptr);

  std::string result = "{\"storages\": [";
  for (size_t i = 0; i < g_storages.size(); ++i) {
    const auto& s = g_storages[i];
    unsigned long long total = 0;
    unsigned long long avail = 0;
    storage_get_total_space(s.id, &total);
    storage_get_available_space(s.id, &avail);

    char used_pct[16] = "0.0";
    if (total > 0) {
      double pct =
          (1.0 - static_cast<double>(avail) / total)
          * 100.0;
      snprintf(used_pct, sizeof(used_pct),
               "%.1f", pct);
    }

    if (i > 0)
      result += ", ";

    result +=
        "{\"id\": " + std::to_string(s.id) + ", "
        "\"type\": \"" + s.type + "\", "
        "\"state\": \"" + s.state + "\", "
        "\"path\": \"" + s.path + "\", "
        "\"total_bytes\": " +
        std::to_string(total) + ", "
        "\"available_bytes\": " +
        std::to_string(avail) + ", "
        "\"used_percent\": " + used_pct + "}";
  }

  result += "]}";
  return result;
}

}  // namespace cli
}  // namespace tizenclaw
