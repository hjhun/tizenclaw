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

#include "vacuum_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kConfigPath[] =
    "/opt/usr/share/tizenclaw/data/config/robotic_vacuum_config.json";

constexpr const char kUsage[] = R"(Usage:
  robotic-vacuum-cli <subcommand> [options]

Subcommands:
  start    Start cleaning [--mode auto|part|repeat|manual|map]
  stop     Stop cleaning (movement -> idle)
  pause    Pause cleaning
  dock     Return to charging dock (movement -> homing)
  status   Get battery level, movement state, cleaning mode, turbo
  turbo    Set suction power [--level on|off|silence]

All output is JSON. Credentials are read from:
  /opt/usr/share/tizenclaw/data/config/robotic_vacuum_config.json
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
  tizenclaw::cli::VacuumController vc(kConfigPath);

  if (cmd == "start") {
    std::string mode = "auto";
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--mode")
        mode = argv[i + 1];
    }
    std::cout << vc.Start(mode) << std::endl;

  } else if (cmd == "stop") {
    std::cout << vc.Stop() << std::endl;

  } else if (cmd == "pause") {
    std::cout << vc.Pause() << std::endl;

  } else if (cmd == "dock") {
    std::cout << vc.Dock() << std::endl;

  } else if (cmd == "status") {
    std::cout << vc.Status() << std::endl;

  } else if (cmd == "turbo") {
    std::string level = "on";
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--level")
        level = argv[i + 1];
    }
    std::cout << vc.SetTurbo(level) << std::endl;

  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
