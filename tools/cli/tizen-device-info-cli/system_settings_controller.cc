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

#include "system_settings_controller.hh"

#include <system_settings.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

std::string GetStr(system_settings_key_e key) {
  char* val = nullptr;
  if (system_settings_get_value_string(
          key, &val) == 0 && val) {
    std::string result(val);
    free(val);
    return result;
  }
  return "";
}

std::string GetBoolStr(system_settings_key_e key) {
  bool val = false;
  if (system_settings_get_value_bool(
          key, &val) == 0)
    return val ? "true" : "false";
  return "null";
}

constexpr const char* kFontSizes[] = {
    "small", "normal", "large", "huge", "giant"};

}  // namespace

std::string
SystemSettingsController::GetSystemSettings() const {
  int fs = 0;
  system_settings_get_value_int(
      SYSTEM_SETTINGS_KEY_FONT_SIZE, &fs);
  const char* fsize =
      (fs >= 0 && fs <= 4) ? kFontSizes[fs]
                           : "unknown";

  return "{\"locale_country\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_LOCALE_COUNTRY) +
         "\", \"locale_language\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_LOCALE_LANGUAGE) +
         "\", \"timezone\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_LOCALE_TIMEZONE) +
         "\", \"time_format_24h\": " +
         GetBoolStr(
             SYSTEM_SETTINGS_KEY_LOCALE_TIMEFORMAT_24HOUR) +
         ", \"device_name\": \"" +
         GetStr(SYSTEM_SETTINGS_KEY_DEVICE_NAME) +
         "\", \"ringtone_path\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_INCOMING_CALL_RINGTONE) +
         "\", \"wallpaper_home\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_WALLPAPER_HOME_SCREEN) +
         "\", \"wallpaper_lock\": \"" +
         GetStr(
             SYSTEM_SETTINGS_KEY_WALLPAPER_LOCK_SCREEN) +
         "\", \"font_type\": \"" +
         GetStr(SYSTEM_SETTINGS_KEY_FONT_TYPE) +
         "\", \"font_size\": \"" + fsize +
         "\", \"motion_enabled\": " +
         GetBoolStr(
             SYSTEM_SETTINGS_KEY_MOTION_ENABLED) +
         "}";
}

}  // namespace cli
}  // namespace tizenclaw
