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

#include "sensor_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-sensor-cli --type <sensor_type>

Sensor types:
  accelerometer, gravity, gyroscope,
  light, proximity, pressure,
  magnetic, orientation
)";

void PrintUsage() {
  std::cerr << kUsage;
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 3) {
    PrintUsage();
    return 1;
  }

  std::string type = "accelerometer";
  for (int i = 1; i < argc - 1; ++i) {
    if (std::string(argv[i]) == "--type")
      type = argv[i + 1];
  }

  tizenclaw::cli::SensorController c;
  std::cout << c.Read(type) << std::endl;

  return 0;
}
