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
#include "health_monitor.hh"

#include <unistd.h>

#include <fstream>
#include <json.hpp>
#include <sstream>

namespace tizenclaw {

HealthMonitor::HealthMonitor()
    : start_time_(std::chrono::steady_clock::now()) {}

void HealthMonitor::IncrementRequestCount() {
  request_count_.fetch_add(1, std::memory_order_relaxed);
}

void HealthMonitor::IncrementErrorCount() {
  error_count_.fetch_add(1, std::memory_order_relaxed);
}

void HealthMonitor::IncrementLlmCallCount() {
  llm_call_count_.fetch_add(1, std::memory_order_relaxed);
}

void HealthMonitor::IncrementToolCallCount() {
  tool_call_count_.fetch_add(1, std::memory_order_relaxed);
}

uint64_t HealthMonitor::GetRequestCount() const {
  return request_count_.load(std::memory_order_relaxed);
}

uint64_t HealthMonitor::GetErrorCount() const {
  return error_count_.load(std::memory_order_relaxed);
}

uint64_t HealthMonitor::GetLlmCallCount() const {
  return llm_call_count_.load(std::memory_order_relaxed);
}

uint64_t HealthMonitor::GetToolCallCount() const {
  return tool_call_count_.load(std::memory_order_relaxed);
}

double HealthMonitor::GetUptimeSeconds() const {
  auto now = std::chrono::steady_clock::now();
  auto duration =
      std::chrono::duration_cast<std::chrono::milliseconds>(now - start_time_);
  return duration.count() / 1000.0;
}

void HealthMonitor::ParseMemoryInfo(int& rss_kb, int& vm_kb) const {
  rss_kb = 0;
  vm_kb = 0;

  std::ifstream f("/proc/self/status");
  if (!f.is_open()) return;

  std::string line;
  while (std::getline(f, line)) {
    if (line.compare(0, 6, "VmRSS:") == 0) {
      std::istringstream iss(line.substr(6));
      iss >> rss_kb;
    } else if (line.compare(0, 7, "VmSize:") == 0) {
      std::istringstream iss(line.substr(7));
      iss >> vm_kb;
    }
  }
}

void HealthMonitor::ParseCpuLoad(double& l1, double& l5, double& l15) const {
  l1 = l5 = l15 = 0.0;

  std::ifstream f("/proc/loadavg");
  if (!f.is_open()) return;

  f >> l1 >> l5 >> l15;
}

int HealthMonitor::GetThreadCount() const {
  std::ifstream f("/proc/self/status");
  if (!f.is_open()) return 0;

  std::string line;
  while (std::getline(f, line)) {
    if (line.compare(0, 8, "Threads:") == 0) {
      std::istringstream iss(line.substr(8));
      int val = 0;
      iss >> val;
      return val;
    }
  }
  return 0;
}

std::string HealthMonitor::GetMetricsJson() const {
  nlohmann::json metrics;

  // Uptime
  double uptime = GetUptimeSeconds();
  int hours = static_cast<int>(uptime) / 3600;
  int minutes = (static_cast<int>(uptime) % 3600) / 60;
  int seconds = static_cast<int>(uptime) % 60;

  metrics["uptime"] = {
      {"seconds", uptime},
      {"formatted", std::to_string(hours) + "h " + std::to_string(minutes) +
                        "m " + std::to_string(seconds) + "s"}};

  // Counters
  metrics["counters"] = {{"requests", GetRequestCount()},
                         {"errors", GetErrorCount()},
                         {"llm_calls", GetLlmCallCount()},
                         {"tool_calls", GetToolCallCount()}};

  // System
  int rss_kb = 0, vm_kb = 0;
  ParseMemoryInfo(rss_kb, vm_kb);
  metrics["memory"] = {{"vm_rss_kb", rss_kb}, {"vm_size_kb", vm_kb}};

  double l1 = 0, l5 = 0, l15 = 0;
  ParseCpuLoad(l1, l5, l15);
  metrics["cpu"] = {{"load_1m", l1}, {"load_5m", l5}, {"load_15m", l15}};

  metrics["threads"] = GetThreadCount();
  metrics["pid"] = getpid();

  return metrics.dump();
}

}  // namespace tizenclaw
