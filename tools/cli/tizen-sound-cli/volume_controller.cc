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

#include "volume_controller.hh"

#include <sound_manager.h>

#include <algorithm>
#include <map>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

// clang-format off
const std::map<std::string, sound_type_e> kTypes = {
  {"system",       SOUND_TYPE_SYSTEM},
  {"notification", SOUND_TYPE_NOTIFICATION},
  {"alarm",        SOUND_TYPE_ALARM},
  {"ringtone",     SOUND_TYPE_RINGTONE},
  {"media",        SOUND_TYPE_MEDIA},
  {"call",         SOUND_TYPE_CALL},
  {"voip",         SOUND_TYPE_VOIP},
};
// clang-format on

}  // namespace

std::string VolumeController::GetVolumes() const {
  std::string r = "{\"action\": \"get\", \"volumes\": {";
  int idx = 0;
  for (const auto& [name, type] : kTypes) {
    int vol = 0;
    int mx = 0;
    if (sound_manager_get_volume(type, &vol) != 0)
      continue;

    sound_manager_get_max_volume(type, &mx);
    if (idx > 0)
      r += ", ";

    r += "\"" + name + "\": {"
         "\"current\": " + std::to_string(vol) +
         ", \"max\": " + std::to_string(mx) + "}";
    ++idx;
  }

  r += "}}";
  return r;
}

std::string VolumeController::SetVolume(
    const std::string& type, int level) {
  auto it = kTypes.find(type);
  if (it == kTypes.end()) {
    return "{\"error\": \"Unknown sound type: " +
           type + "\"}";
  }

  int mx = 0;
  sound_manager_get_max_volume(it->second, &mx);
  int clamped = std::max(0, std::min(level, mx));
  int ret = sound_manager_set_volume(
      it->second, clamped);

  if (ret != 0) {
    return "{\"error\": \"Failed to set volume "
           "(code: " + std::to_string(ret) + ")\"}";
  }

  return "{\"status\": \"success\", "
         "\"action\": \"set\", "
         "\"sound_type\": \"" + type + "\", "
         "\"volume_set\": " +
         std::to_string(clamped) +
         ", \"max_volume\": " +
         std::to_string(mx) + "}";
}

}  // namespace cli
}  // namespace tizenclaw
