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
#ifndef EVENT_BUS_HH
#define EVENT_BUS_HH

#include <atomic>
#include <condition_variable>
#include <deque>
#include <functional>
#include <json.hpp>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace tizenclaw {

// Event types for system-level events
enum class EventType {
  kAppLifecycle,     // launch, resume, pause, terminate
  kNetworkChanged,   // connected, disconnected, type_changed
  kBatteryChanged,   // level_changed, charging_changed
  kDisplayChanged,   // on, off, dim
  kPackageChanged,   // installed, uninstalled, updated
  kMemoryWarning,    // low memory warning
  kSystemSetting,    // language, timezone, silent mode
  kUsbChanged,       // USB connection state
  kBluetoothChanged, // Bluetooth state
  kLocationChanged,  // GPS/NPS/location enable state
  kRecentApp,        // recently used app history
  kCustom            // user-defined / plugin events
};

// A system event published by event sources
struct SystemEvent {
  EventType type;
  std::string source;      // event source name (e.g. "battery")
  std::string name;        // event name (e.g. "battery.level_changed")
  nlohmann::json data;     // event-specific payload
  int64_t timestamp;       // epoch milliseconds
  std::string plugin_id;   // RPK ID or "builtin"
};

using EventCallback = std::function<void(const SystemEvent&)>;

// Event source descriptor loaded from
// event_source.md plugin descriptors
struct EventSourceDescriptor {
  std::string name;
  std::string plugin_id;   // RPK ID or "builtin"
  std::string type;        // "event_source"
  std::string version;
  std::string collect_method;  // "native", "poll", "script"
  int poll_interval_sec = 0;
  struct EventSchema {
    std::string name;
    std::string description;
    nlohmann::json data_schema;
  };
  std::vector<EventSchema> events;
};

// Singleton event bus with pub/sub pattern.
// Thread-safe, bounded queue, async dispatch.
class EventBus {
 public:
  static EventBus& GetInstance();

  // Start/stop the dispatch loop
  void Start();
  void Stop();

  // Publish an event (thread-safe, non-blocking)
  void Publish(SystemEvent event);

  // Subscribe to specific event type
  int Subscribe(EventType type, EventCallback callback);

  // Subscribe to all events
  int SubscribeAll(EventCallback callback);

  // Unsubscribe by subscription ID
  void Unsubscribe(int subscription_id);

  // Plugin management
  void RegisterEventSource(const EventSourceDescriptor& desc);
  void UnregisterEventSource(const std::string& source_name);
  [[nodiscard]] std::vector<EventSourceDescriptor> ListEventSources() const;

  // Load event source plugins from directory
  void LoadPlugins(const std::string& events_dir);

 private:
  EventBus();
  ~EventBus();
  EventBus(const EventBus&) = delete;
  EventBus& operator=(const EventBus&) = delete;

  // Internal dispatch loop
  void DispatchLoop();

  // Bounded event queue
  std::deque<SystemEvent> queue_;
  std::mutex queue_mutex_;
  std::condition_variable queue_cv_;
  static constexpr size_t kMaxQueueSize = 100;

  // Subscriptions
  struct Subscription {
    int id;
    EventType type;       // kCustom used as "all" sentinel
    bool match_all;
    EventCallback callback;
  };
  std::vector<Subscription> subscribers_;
  mutable std::mutex sub_mutex_;
  int next_sub_id_ = 1;

  // Registered event sources
  std::vector<EventSourceDescriptor> event_sources_;
  mutable std::mutex sources_mutex_;

  // Dispatch thread
  std::thread dispatch_thread_;
  std::atomic<bool> running_{false};
};

}  // namespace tizenclaw

#endif  // EVENT_BUS_HH
