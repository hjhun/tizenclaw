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
#include "tizen_system_event_adapter.hh"

#include <bundle.h>

#include <string>

#include "../../common/logging.hh"
#include "event_bus.hh"

namespace {

using tizenclaw::EventType;

struct EventMapping {
  const char* system_event;
  const char* event_key;
  EventType type;
  const char* event_name;
};

// Map from Tizen system event → EventBus event
constexpr EventMapping kEventMappings[] = {
    {SYSTEM_EVENT_BATTERY_CHARGER_STATUS,
     EVENT_KEY_BATTERY_CHARGER_STATUS,
     EventType::kBatteryChanged,
     "battery.charger_status"},
    {SYSTEM_EVENT_BATTERY_LEVEL_STATUS,
     EVENT_KEY_BATTERY_LEVEL_STATUS,
     EventType::kBatteryChanged,
     "battery.level_status"},
    {SYSTEM_EVENT_WIFI_STATE,
     EVENT_KEY_WIFI_STATE,
     EventType::kNetworkChanged,
     "network.wifi_state"},
    {SYSTEM_EVENT_BT_STATE,
     EVENT_KEY_BT_STATE,
     EventType::kBluetoothChanged,
     "bluetooth.state"},
    {SYSTEM_EVENT_DISPLAY_STATE,
     EVENT_KEY_DISPLAY_STATE,
     EventType::kDisplayChanged,
     "display.state"},
    {SYSTEM_EVENT_USB_STATUS,
     EVENT_KEY_USB_STATUS,
     EventType::kUsbChanged,
     "usb.status"},
    {SYSTEM_EVENT_LOW_MEMORY,
     EVENT_KEY_LOW_MEMORY,
     EventType::kMemoryWarning,
     "memory.low"},
    {SYSTEM_EVENT_NETWORK_STATUS,
     EVENT_KEY_NETWORK_STATUS,
     EventType::kNetworkChanged,
     "network.status"},
    {SYSTEM_EVENT_LANGUAGE_SET,
     EVENT_KEY_LANGUAGE_SET,
     EventType::kSystemSetting,
     "system.language"},
    {SYSTEM_EVENT_REGION_FORMAT,
     EVENT_KEY_REGION_FORMAT,
     EventType::kSystemSetting,
     "system.region_format"},
    {SYSTEM_EVENT_SILENT_MODE,
     EVENT_KEY_SILENT_MODE,
     EventType::kSystemSetting,
     "system.silent_mode"},
    {SYSTEM_EVENT_LOCATION_ENABLE_STATE,
     EVENT_KEY_LOCATION_ENABLE_STATE,
     EventType::kLocationChanged,
     "location.enable_state"},
    {SYSTEM_EVENT_GPS_ENABLE_STATE,
     EVENT_KEY_GPS_ENABLE_STATE,
     EventType::kLocationChanged,
     "location.gps_state"},
};

// Resolve EventType and event_name from system event
const EventMapping* FindMapping(
    const char* event_name) {
  for (const auto& m : kEventMappings) {
    if (std::string(m.system_event) == event_name)
      return &m;
  }
  return nullptr;
}

// Extract string value from bundle by key
std::string GetBundleValue(
    bundle* b, const char* key) {
  char* val = nullptr;
  if (bundle_get_str(b, key, &val) == 0 && val)
    return val;
  return {};
}

}  // namespace

namespace tizenclaw {

TizenSystemEventAdapter::~TizenSystemEventAdapter() {
  Stop();
}

void TizenSystemEventAdapter::Start() {
  if (started_) return;

  LOG(DEBUG) << "TizenSystemEventAdapter: "
             << "registering "
             << (sizeof(kEventMappings) /
                 sizeof(kEventMappings[0]))
             << " system event mappings";

  for (const auto& m : kEventMappings) {
    RegisterSystemEvent(m.system_event);
  }

  // Boot completed (no key/value pair)
  RegisterSystemEvent(SYSTEM_EVENT_BOOT_COMPLETED);

  started_ = true;
  LOG(INFO) << "TizenSystemEventAdapter: started "
            << "with " << handlers_.size()
            << " event handlers";
}

void TizenSystemEventAdapter::Stop() {
  if (!started_) return;

  for (auto handler : handlers_) {
    if (handler)
      event_remove_event_handler(handler);
  }
  handlers_.clear();
  started_ = false;
  LOG(INFO) << "TizenSystemEventAdapter: stopped";
}

std::string TizenSystemEventAdapter::GetName() const {
  return "TizenSystemEventAdapter";
}

void TizenSystemEventAdapter::RegisterSystemEvent(
    const char* event_name) {
  event_handler_h handler = nullptr;
  int ret = event_add_event_handler(
      event_name, OnSystemEvent, this, &handler);
  if (ret != EVENT_ERROR_NONE) {
    LOG(ERROR) << "TizenSystemEventAdapter: failed "
               << "to register '" << event_name
               << "', error=" << ret;
    return;
  }
  handlers_.push_back(handler);
  LOG(DEBUG) << "TizenSystemEventAdapter: "
             << "registered event '" << event_name
             << "'";
}

void TizenSystemEventAdapter::OnSystemEvent(
    const char* event_name,
    bundle* event_data,
    void* user_data) {
  if (!event_name || !user_data) return;

  LOG(DEBUG) << "TizenSystemEventAdapter: "
             << "received system event '"
             << event_name << "'";

  SystemEvent ev;
  ev.source = "tizen_system";
  ev.plugin_id = "builtin";

  // Check for boot completed (no mapping)
  std::string ev_name_str(event_name);
  if (ev_name_str == SYSTEM_EVENT_BOOT_COMPLETED) {
    ev.type = EventType::kCustom;
    ev.name = "system.boot_completed";
    LOG(DEBUG) << "TizenSystemEventAdapter: "
               << "publishing boot_completed";
    EventBus::GetInstance().Publish(std::move(ev));
    return;
  }

  const auto* mapping = FindMapping(event_name);
  if (!mapping) {
    LOG(DEBUG) << "TizenSystemEventAdapter: "
               << "no mapping found for '"
               << event_name << "', ignoring";
    return;
  }

  ev.type = mapping->type;
  ev.name = mapping->event_name;

  // Extract the value from bundle
  if (event_data && mapping->event_key) {
    std::string value =
        GetBundleValue(event_data, mapping->event_key);
    if (!value.empty()) {
      ev.data["value"] = value;
      ev.data["key"] = mapping->event_key;
    }
  }

  LOG(DEBUG) << "TizenSystemEventAdapter: "
             << "publishing event '"
             << ev.name << "'"
             << (ev.data.contains("value")
                 ? ", value=" +
                   ev.data["value"].get<std::string>()
                 : "");

  EventBus::GetInstance().Publish(std::move(ev));
}

}  // namespace tizenclaw
