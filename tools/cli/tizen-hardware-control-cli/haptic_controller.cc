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

#include "haptic_controller.hh"

#include <device/haptic.h>
#include <unistd.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string HapticController::Vibrate(int duration_ms) {
  haptic_device_h handle = nullptr;
  if (device_haptic_open(0, &handle) != 0)
    return "{\"error\": \"Failed to open haptic\"}";

  haptic_effect_h effect = nullptr;
  int ret = device_haptic_vibrate(
      handle, duration_ms, 100, &effect);
  if (ret != 0) {
    device_haptic_close(handle);
    return "{\"error\": \"Failed to vibrate (code: " +
           std::to_string(ret) + ")\"}";
  }

  usleep(duration_ms * 1000);
  device_haptic_stop(handle, effect);
  device_haptic_close(handle);

  return "{\"status\": \"success\", "
         "\"duration_ms\": " +
         std::to_string(duration_ms) +
         ", \"message\": \"Device vibrated\"}";
}

}  // namespace cli
}  // namespace tizenclaw
