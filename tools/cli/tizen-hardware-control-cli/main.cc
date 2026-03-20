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
#include "haptic_controller.hh"
#include "led_controller.hh"
#include "power_controller.hh"

#include <cstdlib>
#include <iostream>
#include <string>

namespace {

void PrintUsage() {
  std::cerr
      << "Usage:\n"
      << "  tizen-hardware-control-cli haptic"
      << " [--duration <ms>]\n"
      << "  tizen-hardware-control-cli led"
      << " --action on|off [--brightness N]\n"
      << "  tizen-hardware-control-cli power"
      << " --action lock|unlock --resource display|cpu\n"
      << "  tizen-hardware-control-cli feedback"
      << " --pattern <NAME>\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string cmd = argv[1];

  if (cmd == "haptic") {
    int dur = 500;
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--duration")
        dur = std::atoi(argv[i + 1]);
    }
    tizenclaw::cli::HapticController c;
    std::cout << c.Vibrate(dur) << std::endl;
  } else if (cmd == "led") {
    std::string action = "on";
    int brightness = -1;
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--action")
        action = argv[i + 1];
      if (std::string(argv[i]) == "--brightness")
        brightness = std::atoi(argv[i + 1]);
    }
    tizenclaw::cli::LedController c;
    std::cout << c.Control(action, brightness) << std::endl;
  } else if (cmd == "power") {
    std::string action = "lock";
    std::string resource = "display";
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--action")
        action = argv[i + 1];
      if (std::string(argv[i]) == "--resource")
        resource = argv[i + 1];
    }
    tizenclaw::cli::PowerController c;
    std::cout << c.Control(action, resource) << std::endl;
  } else if (cmd == "feedback") {
    std::string pattern = "TAP";
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--pattern")
        pattern = argv[i + 1];
    }
    tizenclaw::cli::FeedbackController c;
    std::cout << c.Play(pattern) << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
