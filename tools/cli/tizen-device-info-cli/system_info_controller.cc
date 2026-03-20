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

#include "system_info_controller.hh"

#include <system_info.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

std::string GetString(const char* key) {
  char* val = nullptr;
  if (system_info_get_platform_string(key, &val) == 0 && val) {
    std::string result(val);
    free(val);
    return result;
  }
  return "";
}

int GetInt(const char* key) {
  int val = 0;
  system_info_get_platform_int(key, &val);
  return val;
}

bool GetBool(const char* key) {
  bool val = false;
  system_info_get_platform_bool(key, &val);
  return val;
}

std::string BoolStr(bool v) {
  return v ? "true" : "false";
}

}  // namespace

std::string SystemInfoController::GetSystemInfo() const {
  auto model = GetString(
      "http://tizen.org/system/model_name");
  auto platform = GetString(
      "http://tizen.org/system/platform.name");
  auto version = GetString(
      "http://tizen.org/feature/platform.version");
  auto build = GetString(
      "http://tizen.org/system/build.string");
  auto build_type = GetString(
      "http://tizen.org/system/build.type");
  auto mfr = GetString(
      "http://tizen.org/system/manufacturer");
  auto cpu = GetString(
      "http://tizen.org/feature/platform.core.cpu.arch");

  int sw = GetInt(
      "http://tizen.org/feature/screen.width");
  int sh = GetInt(
      "http://tizen.org/feature/screen.height");
  int dpi = GetInt(
      "http://tizen.org/feature/screen.dpi");

  bool bt = GetBool(
      "http://tizen.org/feature/network.bluetooth");
  bool wifi = GetBool(
      "http://tizen.org/feature/network.wifi");
  bool gps = GetBool(
      "http://tizen.org/feature/location.gps");
  bool cam = GetBool(
      "http://tizen.org/feature/camera");
  bool nfc = GetBool(
      "http://tizen.org/feature/network.nfc");
  bool accel = GetBool(
      "http://tizen.org/feature/sensor.accelerometer");
  bool baro = GetBool(
      "http://tizen.org/feature/sensor.barometer");
  bool gyro = GetBool(
      "http://tizen.org/feature/sensor.gyroscope");

  return "{\"model_name\": \"" + model + "\", "
         "\"platform_name\": \"" + platform + "\", "
         "\"platform_version\": \"" + version + "\", "
         "\"build_string\": \"" + build + "\", "
         "\"build_type\": \"" + build_type + "\", "
         "\"manufacturer\": \"" + mfr + "\", "
         "\"cpu_arch\": \"" + cpu + "\", "
         "\"screen_width\": " + std::to_string(sw) + ", "
         "\"screen_height\": " + std::to_string(sh) + ", "
         "\"screen_dpi\": " + std::to_string(dpi) + ", "
         "\"features\": {"
         "\"bluetooth\": " + BoolStr(bt) + ", "
         "\"wifi\": " + BoolStr(wifi) + ", "
         "\"gps\": " + BoolStr(gps) + ", "
         "\"camera\": " + BoolStr(cam) + ", "
         "\"nfc\": " + BoolStr(nfc) + ", "
         "\"accelerometer\": " + BoolStr(accel) + ", "
         "\"barometer\": " + BoolStr(baro) + ", "
         "\"gyroscope\": " + BoolStr(gyro) +
         "}}";
}

}  // namespace cli
}  // namespace tizenclaw
