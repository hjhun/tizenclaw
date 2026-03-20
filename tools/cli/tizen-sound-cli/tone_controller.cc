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

#include "tone_controller.hh"

#include <sound_manager.h>
#include <tone_player.h>
#include <unistd.h>

#include <algorithm>
#include <map>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

// clang-format off
const std::map<std::string, tone_type_e> kTones = {
  {"DTMF_0",       TONE_TYPE_DTMF_0},
  {"DTMF_1",       TONE_TYPE_DTMF_1},
  {"SUP_DIAL",     TONE_TYPE_SUP_DIAL},
  {"SUP_BUSY",     TONE_TYPE_SUP_BUSY},
  {"SUP_RINGTONE", TONE_TYPE_SUP_RINGTONE},
  {"PROP_BEEP",    TONE_TYPE_PROP_BEEP},
  {"PROP_ACK",     TONE_TYPE_PROP_ACK},
  {"PROP_NACK",    TONE_TYPE_PROP_NACK},
  {"PROP_PROMPT",  TONE_TYPE_PROP_PROMPT},
  {"PROP_BEEP2",   TONE_TYPE_PROP_BEEP2},
};
// clang-format on

}  // namespace

std::string ToneController::Play(
    const std::string& tone_name,
    int duration_ms) {
  std::string upper = tone_name;
  std::transform(upper.begin(), upper.end(),
                 upper.begin(), ::toupper);

  auto it = kTones.find(upper);
  if (it == kTones.end()) {
    return "{\"error\": \"Unknown tone: " +
           tone_name + "\"}";
  }

  sound_stream_info_h stream = nullptr;
  if (sound_manager_create_stream_information(
          SOUND_STREAM_TYPE_MEDIA,
          nullptr, nullptr, &stream) != 0)
    return "{\"error\": \"Failed to create stream\"}";

  int id = 0;
  int ret = tone_player_start_new(
      it->second, stream, duration_ms, &id);
  if (ret != 0) {
    sound_manager_destroy_stream_information(stream);
    return "{\"error\": \"Failed to play (code: " +
           std::to_string(ret) + ")\"}";
  }

  usleep((duration_ms + 100) * 1000);
  tone_player_stop(id);
  sound_manager_destroy_stream_information(stream);

  return "{\"status\": \"success\", "
         "\"tone\": \"" + upper + "\", "
         "\"duration_ms\": " +
         std::to_string(duration_ms) + "}";
}

}  // namespace cli
}  // namespace tizenclaw
