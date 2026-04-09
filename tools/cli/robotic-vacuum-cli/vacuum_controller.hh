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

#ifndef TIZENCLAW_CLI_VACUUM_CONTROLLER_HH_
#define TIZENCLAW_CLI_VACUUM_CONTROLLER_HH_

#include "smartthings_client.hh"

#include <string>

namespace tizenclaw {
namespace cli {

// High-level controller for Samsung Jet Bot robot vacuum.
// Translates user-facing subcommands into SmartThings capability commands.
// All methods return a valid JSON string for stdout output.
class VacuumController {
 public:
  explicit VacuumController(const std::string& config_path);

  // Start cleaning. mode: auto | part | repeat | manual | map
  // Sends setRobotCleanerCleaningMode + setRobotCleanerMovement("cleaning")
  // in a single API call.
  std::string Start(const std::string& mode);

  // Stop cleaning. Sends setRobotCleanerMovement("idle").
  std::string Stop();

  // Pause cleaning. Sends setRobotCleanerMovement("pause").
  std::string Pause();

  // Return to dock. Sends setRobotCleanerMovement("homing").
  std::string Dock();

  // Query battery, movement, cleaning mode, and turbo state.
  std::string Status();

  // Set suction power. level: on | off | silence
  // Sends setRobotCleanerTurboMode(level).
  std::string SetTurbo(const std::string& level);

 private:
  SmartThingsClient client_;
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_VACUUM_CONTROLLER_HH_
