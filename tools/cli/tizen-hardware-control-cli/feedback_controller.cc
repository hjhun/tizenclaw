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

#include "feedback_controller.hh"

#include <feedback.h>

#include <algorithm>
#include <map>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

// clang-format off
const std::map<std::string, feedback_pattern_e>
    kPatterns = {
  {"TAP",           FEEDBACK_PATTERN_TAP},
  {"SIP",           FEEDBACK_PATTERN_SIP},
  {"KEY0",          FEEDBACK_PATTERN_KEY0},
  {"KEY1",          FEEDBACK_PATTERN_KEY1},
  {"KEY2",          FEEDBACK_PATTERN_KEY2},
  {"KEY3",          FEEDBACK_PATTERN_KEY3},
  {"KEY4",          FEEDBACK_PATTERN_KEY4},
  {"KEY5",          FEEDBACK_PATTERN_KEY5},
  {"KEY6",          FEEDBACK_PATTERN_KEY6},
  {"KEY7",          FEEDBACK_PATTERN_KEY7},
  {"KEY8",          FEEDBACK_PATTERN_KEY8},
  {"KEY9",          FEEDBACK_PATTERN_KEY9},
  {"HOLD",          FEEDBACK_PATTERN_HOLD},
  {"HW_TAP",        FEEDBACK_PATTERN_HW_TAP},
  {"HW_HOLD",       FEEDBACK_PATTERN_HW_HOLD},
  {"MESSAGE",       FEEDBACK_PATTERN_MESSAGE},
  {"EMAIL",         FEEDBACK_PATTERN_EMAIL},
  {"WAKEUP",        FEEDBACK_PATTERN_WAKEUP},
  {"SCHEDULE",      FEEDBACK_PATTERN_SCHEDULE},
  {"TIMER",         FEEDBACK_PATTERN_TIMER},
  {"GENERAL",       FEEDBACK_PATTERN_GENERAL},
  {"POWERON",       FEEDBACK_PATTERN_POWERON},
  {"POWEROFF",      FEEDBACK_PATTERN_POWEROFF},
  {"CHARGERCONN",   FEEDBACK_PATTERN_CHARGERCONN},
  {"CHARGING_ERROR",
      FEEDBACK_PATTERN_CHARGING_ERROR},
  {"FULLCHARGED",   FEEDBACK_PATTERN_FULLCHARGED},
  {"LOWBATT",       FEEDBACK_PATTERN_LOWBATT},
  {"LOCK",          FEEDBACK_PATTERN_LOCK},
  {"UNLOCK",        FEEDBACK_PATTERN_UNLOCK},
  {"VIBRATION_ON",  FEEDBACK_PATTERN_VIBRATION_ON},
  {"SILENT_OFF",    FEEDBACK_PATTERN_SILENT_OFF},
  {"BT_CONNECTED",  FEEDBACK_PATTERN_BT_CONNECTED},
  {"BT_DISCONNECTED",
      FEEDBACK_PATTERN_BT_DISCONNECTED},
};
// clang-format on

}  // namespace

std::string FeedbackController::Play(
    const std::string& pattern_name) {
  std::string upper = pattern_name;
  std::transform(upper.begin(), upper.end(),
                 upper.begin(), ::toupper);

  auto it = kPatterns.find(upper);
  if (it == kPatterns.end()) {
    return "{\"error\": \"Unknown pattern: " +
           pattern_name + "\"}";
  }

  if (feedback_initialize() != 0)
    return "{\"error\": \"feedback_initialize failed\"}";

  int ret = feedback_play(it->second);
  feedback_deinitialize();

  if (ret != 0) {
    return "{\"error\": \"Failed to play (code: " +
           std::to_string(ret) + ")\"}";
  }

  return "{\"status\": \"success\", "
         "\"pattern\": \"" + upper + "\", "
         "\"message\": \"Played " + upper + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
