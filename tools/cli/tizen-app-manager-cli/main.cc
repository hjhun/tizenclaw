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

#include "app_control_controller.hh"
#include "list_apps_controller.hh"
#include "package_info_controller.hh"
#include "terminate_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-app-manager-cli <subcommand>

Subcommands:
  list          List installed UI apps
  terminate     Terminate a running app
  launch        Launch an app via AppControl
  package-info  Get package information
)";

void PrintUsage() {
  std::cerr << kUsage;
}

std::string GetArg(int argc, char* argv[],
                   const std::string& key) {
  for (int i = 2; i < argc - 1; ++i) {
    if (std::string(argv[i]) == key)
      return argv[i + 1];
  }
  return "";
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string cmd = argv[1];

  if (cmd == "list") {
    tizenclaw::cli::ListAppsController c;
    std::cout << c.ListApps() << std::endl;
  } else if (cmd == "terminate") {
    std::string id = GetArg(argc, argv, "--app-id");
    if (id.empty()) {
      std::cerr << "--app-id required\n";
      return 1;
    }
    tizenclaw::cli::TerminateController c;
    std::cout << c.Terminate(id) << std::endl;
  } else if (cmd == "launch") {
    std::string id = GetArg(argc, argv, "--app-id");
    std::string op = GetArg(argc, argv, "--operation");
    std::string uri = GetArg(argc, argv, "--uri");
    std::string mime = GetArg(argc, argv, "--mime");
    tizenclaw::cli::AppControlController c;
    std::cout << c.Launch(id, op, uri, mime) << std::endl;
  } else if (cmd == "package-info") {
    std::string id = GetArg(argc, argv, "--package-id");
    if (id.empty()) {
      std::cerr << "--package-id required\n";
      return 1;
    }
    tizenclaw::cli::PackageInfoController c;
    std::cout << c.GetInfo(id) << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
