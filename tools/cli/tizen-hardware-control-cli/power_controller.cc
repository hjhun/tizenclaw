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

#include "power_controller.hh"

#include <device/power.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string PowerController::Control(
    const std::string& action,
    const std::string& resource) {
  power_lock_e lock =
      (resource == "cpu") ? POWER_LOCK_CPU
                          : POWER_LOCK_DISPLAY;

  if (action == "lock") {
    if (device_power_request_lock(lock, 0) != 0) {
      return "{\"error\": \"Failed to request " +
             resource + " lock\"}";
    }

    return "{\"status\": \"success\", "
           "\"action\": \"lock\", "
           "\"resource\": \"" + resource + "\", "
           "\"message\": \"" + resource +
           " lock acquired\"}";
  }

  if (device_power_release_lock(lock) != 0) {
    return "{\"error\": \"Failed to release " +
           resource + " lock\"}";
  }

  return "{\"status\": \"success\", "
         "\"action\": \"unlock\", "
         "\"resource\": \"" + resource + "\", "
         "\"message\": \"" + resource +
         " lock released\"}";
}

}  // namespace cli
}  // namespace tizenclaw
