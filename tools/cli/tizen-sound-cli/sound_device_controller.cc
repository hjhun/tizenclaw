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

#include "sound_device_controller.hh"

#include <sound_manager.h>

#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kTypeNames[] = {
    "builtin_speaker", "builtin_receiver", "builtin_mic",
    "audio_jack", "bluetooth_media", "hdmi",
    "forwarding", "usb_audio", "bluetooth_voice"};

constexpr const char* kDirNames[] = {"input", "output", "both"};

}  // namespace

std::string SoundDeviceController::GetDevices() const {
  sound_device_list_h list = nullptr;
  if (sound_manager_get_device_list(
          SOUND_DEVICE_ALL_MASK, &list) != 0)
    return "{\"error\": \"Failed to get device list\"}";

  std::string result = "{\"devices\": [";
  int count = 0;
  sound_device_h dev = nullptr;

  while (sound_manager_get_next_device(list, &dev) == 0) {
    sound_device_type_e type;
    int id = 0;
    sound_device_io_direction_e dir;
    char* name = nullptr;

    sound_manager_get_device_type(dev, &type);
    sound_manager_get_device_id(dev, &id);
    sound_manager_get_device_io_direction(dev, &dir);
    sound_manager_get_device_name(dev, &name);

    if (count > 0)
      result += ", ";

    const char* type_str =
        (type <= 8) ? kTypeNames[type] : "unknown";
    const char* dir_str =
        (dir <= 2) ? kDirNames[dir] : "unknown";

    result += "{\"id\": " + std::to_string(id) +
              ", \"type\": \"" + type_str + "\"" +
              ", \"name\": \"" +
              (name ? name : "unknown") + "\"" +
              ", \"direction\": \"" + dir_str + "\"}";
    count++;
  }

  sound_manager_free_device_list(list);
  result += "], \"count\": " +
            std::to_string(count) + "}";

  return result;
}

}  // namespace cli
}  // namespace tizenclaw
