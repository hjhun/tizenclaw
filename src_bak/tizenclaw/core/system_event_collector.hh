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
#ifndef SYSTEM_EVENT_COLLECTOR_HH
#define SYSTEM_EVENT_COLLECTOR_HH

#include <atomic>
#include <string>
#include <thread>

#include "event_bus.hh"

namespace tizenclaw {

// Collects system events from /proc and local
// sensors, publishes them to EventBus.
// Uses polling for portability (works in
// emulator and chroot without Tizen C-API).
class SystemEventCollector {
 public:
  SystemEventCollector();
  ~SystemEventCollector();

  void Start();
  void Stop();

 private:
  // Periodic collection loop
  void CollectLoop();

  // Collect individual metrics
  void CollectBattery();
  void CollectMemory();
  void CollectNetwork();

  // Previous values for change detection
  int prev_battery_level_ = -1;
  bool prev_charging_ = false;
  std::string prev_network_status_;
  int prev_memory_pct_ = -1;

  std::thread collect_thread_;
  std::atomic<bool> running_{false};

  // Collection interval (seconds)
  static constexpr int kCollectIntervalSec = 30;
};

}  // namespace tizenclaw

#endif  // SYSTEM_EVENT_COLLECTOR_HH
