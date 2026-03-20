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

#include "sound_device_controller.hh"
#include "tone_controller.hh"
#include "volume_controller.hh"

#include <cstdlib>
#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-sound-cli <subcommand> [options]

Subcommands:
  volume  [set --type <TYPE> --level <N>]
  devices List audio devices
  tone    [--name <NAME>] [--duration <ms>]
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

  if (cmd == "volume") {
    tizenclaw::cli::VolumeController c;
    if (argc >= 3 && std::string(argv[2]) == "set") {
      std::string type;
      int level = 0;
      for (int i = 3; i < argc - 1; ++i) {
        if (std::string(argv[i]) == "--type")
          type = argv[i + 1];
        if (std::string(argv[i]) == "--level")
          level = std::atoi(argv[i + 1]);
      }
      std::cout << c.SetVolume(type, level) << std::endl;
    } else {
      std::cout << c.GetVolumes() << std::endl;
    }
  } else if (cmd == "devices") {
    tizenclaw::cli::SoundDeviceController c;
    std::cout << c.GetDevices() << std::endl;
  } else if (cmd == "tone") {
    std::string name = "PROP_BEEP";
    int dur = 500;
    for (int i = 2; i < argc - 1; ++i) {
      if (std::string(argv[i]) == "--name")
        name = argv[i + 1];
      if (std::string(argv[i]) == "--duration")
        dur = std::atoi(argv[i + 1]);
    }
    tizenclaw::cli::ToneController c;
    std::cout << c.Play(name, dur) << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
