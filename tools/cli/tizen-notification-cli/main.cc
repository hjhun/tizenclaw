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

#include "alarm_controller.hh"
#include "notification_controller.hh"

#include <iostream>
#include <string>

namespace {

constexpr const char kUsage[] = R"(Usage:
  tizen-notification-cli <subcommand> [options]

Subcommands:
  notify  Send a notification
  alarm   Schedule an alarm
)";

void PrintUsage() {
  std::cerr << kUsage;
}

std::string GetArg(int argc, char* argv[],
                   const std::string& key,
                   const std::string& default_val = "") {
  for (int i = 2; i < argc - 1; ++i) {
    if (std::string(argv[i]) == key)
      return argv[i + 1];
  }
  return default_val;
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    PrintUsage();
    return 1;
  }

  std::string cmd = argv[1];

  if (cmd == "notify") {
    std::string title = GetArg(argc, argv, "--title", "TizenClaw");
    std::string body = GetArg(argc, argv, "--body", "Hello!");
    tizenclaw::cli::NotificationController c;
    std::cout << c.Send(title, body) << std::endl;
  } else if (cmd == "alarm") {
    std::string id = GetArg(argc, argv, "--app-id");
    std::string dt = GetArg(argc, argv, "--datetime");
    if (id.empty() || dt.empty()) {
      std::cerr << "--app-id and --datetime required\n";
      return 1;
    }
    tizenclaw::cli::AlarmController c;
    std::cout << c.Schedule(id, dt) << std::endl;
  } else {
    PrintUsage();
    return 1;
  }

  return 0;
}
