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

#include "thermal_controller.hh"

#include <device/power.h>

#include <cstdio>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kZoneNames[] = {
    "ap", "cp", "battery"};

}  // namespace

std::string
ThermalController::GetThermalInfo() const {
  std::string result = "{\"thermal\": {";
  int found = 0;

  for (int i = 0; i < 3; ++i) {
    char path[128];
    snprintf(path, sizeof(path),
             "/sys/class/thermal/"
             "thermal_zone%d/temp", i);

    FILE* f = fopen(path, "r");
    if (!f)
      continue;

    int temp = 0;
    if (fscanf(f, "%d", &temp) == 1) {
      if (found > 0)
        result += ", ";

      char buf[32];
      snprintf(buf, sizeof(buf),
               "%.1f", temp / 1000.0);
      result += "\"" +
                std::string(kZoneNames[i]) +
                "\": {\"celsius\": " + buf + "}";
      found++;
    }

    fclose(f);
  }

  result += "}}";

  if (found == 0)
    return "{\"error\": \"No thermal data\"}";

  return result;
}

}  // namespace cli
}  // namespace tizenclaw
