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

#include "battery_controller.hh"
#include "display_info_controller.hh"
#include "runtime_info_controller.hh"
#include "storage_controller.hh"
#include "system_info_controller.hh"
#include "system_settings_controller.hh"
#include "thermal_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-device-info-cli <subcommand>

Subcommands:
  battery      Battery info
  system-info  System hardware info
  runtime      CPU/memory usage
  storage      Storage devices/space
  thermal      Temperature sensors
  display      Display state/brightness
  settings     System settings
)";

void PrintUsage() {
  std::cerr << kUsage;
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string cmd = argv[1];

  if (cmd == "battery") {
    tizenclaw::cli::BatteryController c;
    std::cout << c.GetBatteryInfo() << std::endl;
  } else if (cmd == "system-info") {
    tizenclaw::cli::SystemInfoController c;
    std::cout << c.GetSystemInfo() << std::endl;
  } else if (cmd == "runtime") {
    tizenclaw::cli::RuntimeInfoController c;
    std::cout << c.GetRuntimeInfo() << std::endl;
  } else if (cmd == "storage") {
    tizenclaw::cli::StorageController c;
    std::cout << c.GetStorageInfo() << std::endl;
  } else if (cmd == "thermal") {
    tizenclaw::cli::ThermalController c;
    std::cout << c.GetThermalInfo() << std::endl;
  } else if (cmd == "display") {
    tizenclaw::cli::DisplayInfoController c;
    std::cout << c.GetDisplayInfo() << std::endl;
  } else if (cmd == "settings") {
    tizenclaw::cli::SystemSettingsController c;
    std::cout << c.GetSystemSettings() << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
