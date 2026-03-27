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
#include "system_event_collector.hh"

#include <chrono>
#include <fstream>
#include <sstream>
#include <string>

#include "../../common/logging.hh"

namespace tizenclaw {

SystemEventCollector::SystemEventCollector() = default;

SystemEventCollector::~SystemEventCollector() {
  Stop();
}

void SystemEventCollector::Start() {
  if (running_.load()) return;

  running_.store(true);
  collect_thread_ = std::thread(
      &SystemEventCollector::CollectLoop, this);
  LOG(INFO) << "SystemEventCollector started "
            << "(interval=" << kCollectIntervalSec
            << "s)";
}

void SystemEventCollector::Stop() {
  if (!running_.load()) return;

  running_.store(false);
  if (collect_thread_.joinable()) {
    collect_thread_.join();
  }
  LOG(INFO) << "SystemEventCollector stopped";
}

void SystemEventCollector::CollectLoop() {
  // Initial collection
  CollectBattery();
  CollectMemory();
  CollectNetwork();

  while (running_.load()) {
    // Sleep in small increments for
    // responsive shutdown
    for (int i = 0; i < kCollectIntervalSec * 10; ++i) {
      if (!running_.load()) return;
      std::this_thread::sleep_for(
          std::chrono::milliseconds(100));
    }

    CollectBattery();
    CollectMemory();
    CollectNetwork();
  }
}

void SystemEventCollector::CollectBattery() {
  // Try Tizen battery sysfs paths
  // (works on emulator and real devices)
  int level = -1;
  bool charging = false;

  // Try /sys/class/power_supply/battery/
  {
    std::ifstream f(
        "/sys/class/power_supply/battery/capacity");
    if (f.is_open()) {
      f >> level;
    }
  }

  {
    std::ifstream f(
        "/sys/class/power_supply/battery/status");
    if (f.is_open()) {
      std::string status;
      f >> status;
      charging = (status == "Charging" ||
                  status == "Full");
    }
  }

  // Fallback: emulator often uses BAT0
  if (level < 0) {
    std::ifstream f(
        "/sys/class/power_supply/BAT0/capacity");
    if (f.is_open()) {
      f >> level;
    }
  }

  if (level < 0) return;  // No battery info

  // Only publish on change
  if (level != prev_battery_level_ ||
      charging != prev_charging_) {
    SystemEvent event;
    event.type = EventType::kBatteryChanged;
    event.source = "battery";
    event.plugin_id = "builtin";

    if (level != prev_battery_level_) {
      event.name = "battery.level_changed";
      event.data = {{"level", level},
                    {"charging", charging}};
    } else {
      event.name = charging
                       ? "battery.charging_started"
                       : "battery.charging_stopped";
      event.data = {{"level", level},
                    {"charging", charging}};
    }

    EventBus::GetInstance().Publish(
        std::move(event));
    prev_battery_level_ = level;
    prev_charging_ = charging;
  }
}

void SystemEventCollector::CollectMemory() {
  // Parse /proc/meminfo
  std::ifstream f("/proc/meminfo");
  if (!f.is_open()) return;

  long mem_total = 0, mem_available = 0;
  std::string line;
  while (std::getline(f, line)) {
    if (line.compare(0, 9, "MemTotal:") == 0) {
      std::istringstream ss(line.substr(9));
      ss >> mem_total;
    } else if (line.compare(0, 13,
                            "MemAvailable:") == 0) {
      std::istringstream ss(line.substr(13));
      ss >> mem_available;
    }
  }

  if (mem_total <= 0) return;

  int pct = static_cast<int>(
      100 * (mem_total - mem_available) / mem_total);

  // Only publish on significant change (>5%)
  if (std::abs(pct - prev_memory_pct_) >= 5) {
    SystemEvent event;
    event.type = EventType::kMemoryWarning;
    event.source = "memory";
    event.name = pct >= 90 ? "memory.critical"
                 : pct >= 80 ? "memory.warning"
                             : "memory.normal";
    event.data = {
        {"usage_percent", pct},
        {"total_kb", mem_total},
        {"available_kb", mem_available},
        {"level",
         pct >= 90   ? "critical"
         : pct >= 80 ? "warning"
                     : "normal"}};
    event.plugin_id = "builtin";

    EventBus::GetInstance().Publish(
        std::move(event));
    prev_memory_pct_ = pct;
  }
}

void SystemEventCollector::CollectNetwork() {
  // Check network status via /sys/class/net/
  // Look for interfaces that are UP
  std::string status = "disconnected";
  std::string type = "none";

  // Check common interfaces
  for (const auto& iface :
       {"wlan0", "eth0", "usb0"}) {
    std::string path = std::string(
        "/sys/class/net/") + iface + "/operstate";
    std::ifstream f(path);
    if (f.is_open()) {
      std::string state;
      f >> state;
      if (state == "up") {
        status = "connected";
        if (std::string(iface).find("wlan") !=
            std::string::npos) {
          type = "wifi";
        } else if (std::string(iface).find("eth") !=
                   std::string::npos) {
          type = "ethernet";
        } else {
          type = "usb";
        }
        break;
      }
    }
  }

  // Only publish on change
  if (status != prev_network_status_) {
    SystemEvent event;
    event.type = EventType::kNetworkChanged;
    event.source = "network";
    event.name = status == "connected"
                     ? "network.connected"
                     : "network.disconnected";
    event.data = {{"status", status},
                  {"type", type}};
    event.plugin_id = "builtin";

    EventBus::GetInstance().Publish(
        std::move(event));
    prev_network_status_ = status;
  }
}

}  // namespace tizenclaw
