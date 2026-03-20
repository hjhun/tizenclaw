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

#include "battery_controller.hh"

#include <device/battery.h>

#include <string>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kLevelNames[] = {
    "empty", "critical", "low", "high", "full"};

}  // namespace

std::string BatteryController::GetBatteryInfo() const {
  int percent = 0;
  if (device_battery_get_percent(&percent) != 0)
    return "{\"error\": \"Failed to get percent\"}";

  bool charging = false;
  device_battery_is_charging(&charging);

  device_battery_level_e level;
  device_battery_get_level_status(&level);

  const char* level_str =
      (level >= 0 && level <= 4)
          ? kLevelNames[level]
          : "unknown";

  return "{\"status\": \"success\", "
         "\"percent\": " +
         std::to_string(percent) + ", "
         "\"is_charging\": " +
         std::string(charging ? "true" : "false") +
         ", \"level_status\": \"" +
         level_str + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
