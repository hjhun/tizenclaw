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

#include "display_info_controller.hh"

#include <device/display.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string DisplayInfoController::GetDisplayInfo() const {
  int num = 0;
  if (device_display_get_numbers(&num) != 0)
    return "{\"error\": \"Failed to get display count\"}";

  display_state_e state;
  device_display_get_state(&state);
  const char* states[] = {"normal", "dim", "off"};
  const char* st = (state <= 2) ? states[state] : "unknown";

  std::string result = "{\"num_displays\": " + std::to_string(num) +
    ", \"state\": \"" + st + "\", \"displays\": [";
  for (int i = 0; i < num; ++i) {
    if (i > 0) result += ", ";
    result += "{\"index\": " + std::to_string(i) + "}";
  }
  result += "]}";
  return result;
}

}  // namespace cli
}  // namespace tizenclaw
