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

#include "bluetooth_controller.hh"
#include "data_usage_controller.hh"
#include "network_controller.hh"
#include "wifi_controller.hh"

#include <iostream>
#include <string>

namespace {

void PrintUsage() {
  std::cerr << "Usage: tizen-network-info-cli <subcommand>\n"
            << "  network     Network connection info\n"
            << "  wifi        Wi-Fi status and AP info\n"
            << "  bluetooth   Bluetooth adapter info\n"
            << "  data-usage  Data usage statistics\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string cmd = argv[1];

  if (cmd == "network") {
    tizenclaw::cli::NetworkController c;
    std::cout << c.GetNetworkInfo() << std::endl;
  } else if (cmd == "wifi") {
    tizenclaw::cli::WifiController c;
    std::cout << c.GetWifiInfo() << std::endl;
  } else if (cmd == "bluetooth") {
    tizenclaw::cli::BluetoothController c;
    std::cout << c.GetInfo() << std::endl;
  } else if (cmd == "data-usage") {
    tizenclaw::cli::DataUsageController c;
    std::cout << c.GetDataUsage() << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
