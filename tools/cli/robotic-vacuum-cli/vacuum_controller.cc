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

#include <string>

namespace tizenclaw {
namespace cli {

namespace {

// Builds a SmartThings commands payload with a single capability command.
std::string SingleCommand(const std::string& capability,
                          const std::string& command,
                          const std::string& argument) {
  return "{\"commands\":[{"
         "\"component\":\"main\","
         "\"capability\":\"" + capability + "\","
         "\"command\":\"" + command + "\","
         "\"arguments\":[\"" + argument + "\"]}]}";
}

// Builds a SmartThings commands payload for two capability commands
// sent in a single request (used by Start to set mode then movement).
std::string DualCommand(const std::string& cap1, const std::string& cmd1,
                        const std::string& arg1,
                        const std::string& cap2, const std::string& cmd2,
                        const std::string& arg2) {
  return "{\"commands\":["
         "{\"component\":\"main\","
         "\"capability\":\"" + cap1 + "\","
         "\"command\":\"" + cmd1 + "\","
         "\"arguments\":[\"" + arg1 + "\"]},"
         "{\"component\":\"main\","
         "\"capability\":\"" + cap2 + "\","
         "\"command\":\"" + cmd2 + "\","
         "\"arguments\":[\"" + arg2 + "\"]}]}";
}

}  // namespace

VacuumController::VacuumController(const std::string& config_path)
    : client_(config_path) {}

std::string VacuumController::Start(const std::string& mode) {
  // Set cleaning mode and start movement in one API round-trip.
  std::string body = DualCommand(
      "robotCleanerCleaningMode", "setRobotCleanerCleaningMode", mode,
      "robotCleanerMovement",    "setRobotCleanerMovement",      "cleaning");

  std::string result = client_.SendCommands(body);

  // Enrich the ok response with action context.
  if (result == "{\"status\":\"ok\"}")
    return "{\"status\":\"ok\",\"action\":\"cleaning\",\"mode\":\"" +
           mode + "\"}";
  return result;
}

std::string VacuumController::Stop() {
  std::string body = SingleCommand(
      "robotCleanerMovement", "setRobotCleanerMovement", "idle");
  std::string result = client_.SendCommands(body);
  if (result == "{\"status\":\"ok\"}")
    return "{\"status\":\"ok\",\"action\":\"idle\"}";
  return result;
}

std::string VacuumController::Pause() {
  std::string body = SingleCommand(
      "robotCleanerMovement", "setRobotCleanerMovement", "pause");
  std::string result = client_.SendCommands(body);
  if (result == "{\"status\":\"ok\"}")
    return "{\"status\":\"ok\",\"action\":\"pause\"}";
  return result;
}

std::string VacuumController::Dock() {
  std::string body = SingleCommand(
      "robotCleanerMovement", "setRobotCleanerMovement", "homing");
  std::string result = client_.SendCommands(body);
  if (result == "{\"status\":\"ok\"}")
    return "{\"status\":\"ok\",\"action\":\"homing\"}";
  return result;
}

std::string VacuumController::Status() {
  return client_.GetStatus();
}

std::string VacuumController::SetTurbo(const std::string& level) {
  std::string body = SingleCommand(
      "robotCleanerTurboMode", "setRobotCleanerTurboMode", level);
  std::string result = client_.SendCommands(body);
  if (result == "{\"status\":\"ok\"}")
    return "{\"status\":\"ok\",\"action\":\"turbo\",\"level\":\"" +
           level + "\"}";
  return result;
}

}  // namespace cli
}  // namespace tizenclaw
