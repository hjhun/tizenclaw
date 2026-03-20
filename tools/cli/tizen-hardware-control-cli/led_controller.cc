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

#include "led_controller.hh"

#include <device/led.h>

#include <algorithm>
#include <string>

namespace tizenclaw {
namespace cli {

std::string LedController::Control(
    const std::string& action, int brightness) {
  int max_b = 0;
  if (device_flash_get_max_brightness(&max_b) != 0)
    return "{\"error\": \"Failed to get max brightness\"}";

  if (action == "off") {
    if (device_flash_set_brightness(0) != 0)
      return "{\"error\": \"Failed to turn off LED\"}";

    return "{\"status\": \"success\", "
           "\"action\": \"off\", "
           "\"message\": \"LED turned off\"}";
  }

  int b = (brightness < 0)
              ? max_b
              : std::max(0, std::min(brightness, max_b));

  if (device_flash_set_brightness(b) != 0)
    return "{\"error\": \"Failed to set brightness\"}";

  return "{\"status\": \"success\", "
         "\"action\": \"on\", "
         "\"brightness\": " + std::to_string(b) +
         ", \"max_brightness\": " +
         std::to_string(max_b) + "}";
}

}  // namespace cli
}  // namespace tizenclaw
