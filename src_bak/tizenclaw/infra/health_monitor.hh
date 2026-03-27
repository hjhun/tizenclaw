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
#ifndef HEALTH_MONITOR_HH
#define HEALTH_MONITOR_HH

#include <atomic>
#include <chrono>
#include <string>

namespace tizenclaw {

// Collects system health metrics for
// monitoring and dashboard display.
class HealthMonitor {
 public:
  HealthMonitor();
  ~HealthMonitor() = default;

  // Increment counters (thread-safe)
  void IncrementRequestCount();
  void IncrementErrorCount();
  void IncrementLlmCallCount();
  void IncrementToolCallCount();

  // Get all metrics as JSON string
  [[nodiscard]] std::string GetMetricsJson() const;

  // Get individual metrics
  [[nodiscard]] uint64_t GetRequestCount() const;
  [[nodiscard]] uint64_t GetErrorCount() const;
  [[nodiscard]] uint64_t GetLlmCallCount() const;
  [[nodiscard]] uint64_t GetToolCallCount() const;
  [[nodiscard]] double GetUptimeSeconds() const;

 private:
  // Parse memory from /proc/self/status
  void ParseMemoryInfo(int& rss_kb, int& vm_kb) const;

  // Parse CPU load from /proc/loadavg
  void ParseCpuLoad(double& l1, double& l5, double& l15) const;

  // Thread count from /proc/self/status
  int GetThreadCount() const;

  // Counters
  std::atomic<uint64_t> request_count_{0};
  std::atomic<uint64_t> error_count_{0};
  std::atomic<uint64_t> llm_call_count_{0};
  std::atomic<uint64_t> tool_call_count_{0};

  // Start time
  std::chrono::steady_clock::time_point start_time_;
};

}  // namespace tizenclaw

#endif  // HEALTH_MONITOR_HH
