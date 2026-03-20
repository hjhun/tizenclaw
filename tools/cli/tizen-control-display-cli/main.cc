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

#include <iostream>
#include <string>
#include <cstdlib>

#include "display_controller.hh"

constexpr const char kUsage[] = R"(Usage:
  tizen-control-display-cli --brightness <value>
  tizen-control-display-cli --info
)";

void PrintUsage() {
  std::cerr << kUsage;
}

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  tizenclaw::cli::DisplayController controller;
  std::string arg = argv[1];

  if (arg == "--brightness" && argc >= 3) {
    int brightness = std::atoi(argv[2]);
    std::cout << controller.SetBrightness(brightness) << std::endl;
  } else if (arg == "--info") {
    std::cout << controller.GetInfo() << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
