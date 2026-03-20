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

#include "runtime_info_controller.hh"

#include <runtime_info.h>

#include <cstdio>
#include <string>

namespace tizenclaw {
namespace cli {

std::string RuntimeInfoController::GetRuntimeInfo() const {
  runtime_memory_info_s mem{};
  runtime_cpu_usage_s cpu{};
  std::string result = "{";

  if (runtime_info_get_system_memory_info(&mem) == 0) {
    double usage = (mem.total > 0) ? (double)mem.used / mem.total * 100.0 : 0;
    char buf[32]; snprintf(buf, sizeof(buf), "%.1f", usage);
    result += "\"memory\": {"
      "\"total_kb\": " + std::to_string(mem.total) + ", "
      "\"used_kb\": " + std::to_string(mem.used) + ", "
      "\"free_kb\": " + std::to_string(mem.free) + ", "
      "\"cache_kb\": " + std::to_string(mem.cache) + ", "
      "\"swap_kb\": " + std::to_string(mem.swap) + ", "
      "\"usage_percent\": " + buf + "}";
  }

  if (runtime_info_get_cpu_usage(&cpu) == 0) {
    char u[32], s[32], n[32], io[32], t[32];
    snprintf(u, 32, "%.1f", cpu.user);
    snprintf(s, 32, "%.1f", cpu.system);
    snprintf(n, 32, "%.1f", cpu.nice);
    snprintf(io, 32, "%.1f", cpu.iowait);
    snprintf(t, 32, "%.1f", cpu.user + cpu.system);
    if (result.size() > 1) result += ", ";
    result += "\"cpu\": {"
      "\"user_percent\": " + std::string(u) + ", "
      "\"system_percent\": " + std::string(s) + ", "
      "\"nice_percent\": " + std::string(n) + ", "
      "\"iowait_percent\": " + std::string(io) + ", "
      "\"total_usage_percent\": " + std::string(t) + "}";
  }

  result += "}";
  return result;
}

}  // namespace cli
}  // namespace tizenclaw
