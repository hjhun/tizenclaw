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

#include "display_controller.hh"

#include <device/display.h>
#include <iostream>
#include <algorithm>

namespace tizenclaw {
namespace cli {

std::string DisplayController::CreateErrorJson(const std::string& error_msg) {
  // Simple JSON string builder (to avoid pulling in json.hpp for basic tools)
  return "{\"status\": \"error\", \"error\": \"" + error_msg + "\"}";
}

std::string DisplayController::SetBrightness(int brightness) {
  int max_brightness = 0;
  if (device_display_get_max_brightness(0, &max_brightness) != 0) {
    return CreateErrorJson("Failed to get max brightness");
  }

  int clamped = std::max(0, std::min(brightness, max_brightness));
  if (device_display_set_brightness(0, clamped) != 0) {
    return CreateErrorJson("Failed to set brightness");
  }

  return "{"
         "\"status\": \"success\", "
         "\"brightness_set\": " + std::to_string(clamped) + ", "
         "\"max_brightness\": " + std::to_string(max_brightness) +
         "}";
}

std::string DisplayController::GetInfo() {
  int max_brightness = 0;
  if (device_display_get_max_brightness(0, &max_brightness) != 0) {
    return CreateErrorJson("Failed to get max brightness");
  }

  int current_brightness = 0;
  if (device_display_get_brightness(0, &current_brightness) != 0) {
    return CreateErrorJson("Failed to get current brightness");
  }

  return "{"
         "\"status\": \"success\", "
         "\"current_brightness\": " + std::to_string(current_brightness) + ", "
         "\"max_brightness\": " + std::to_string(max_brightness) +
         "}";
}

}  // namespace cli
}  // namespace tizenclaw
