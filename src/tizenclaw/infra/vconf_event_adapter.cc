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
#include "vconf_event_adapter.hh"

#include <cstring>
#include <string>

#include "../../common/logging.hh"

namespace tizenclaw {

// Table-driven mapping: vconf key → EventBus event.
// Only high-value keys for LLM device awareness.
const VconfEventAdapter::KeyMapping
    VconfEventAdapter::kMappings[] = {
    // Display
    {"db/setting/Brightness",
     EventType::kDisplayChanged,
     "vconf.display.brightness"},
    {"db/setting/lcd_backlight_normal",
     EventType::kDisplayChanged,
     "vconf.display.lcd_timeout"},
    {"db/setting/brightness_automatic",
     EventType::kDisplayChanged,
     "vconf.display.auto_brightness"},
    // Sound
    {"db/setting/sound/media/sound_volume",
     EventType::kSystemSetting,
     "vconf.sound.media_volume"},
    {"db/setting/sound/call/"
     "ringtone_sound_volume",
     EventType::kSystemSetting,
     "vconf.sound.ringtone_volume"},
    // WiFi
    {"memory/wifi/state",
     EventType::kNetworkChanged,
     "vconf.wifi.state"},
    // Bluetooth
    {"db/bluetooth/status",
     EventType::kBluetoothChanged,
     "vconf.bluetooth.status"},
    // Battery & charging
    {"memory/sysman/battery_capacity",
     EventType::kBatteryChanged,
     "vconf.battery.capacity"},
    {"memory/sysman/charger_status",
     EventType::kBatteryChanged,
     "vconf.battery.charger_status"},
    {"memory/sysman/battery_status_low",
     EventType::kBatteryChanged,
     "vconf.battery.status_low"},
    // USB
    {"memory/sysman/usb_status",
     EventType::kUsbChanged,
     "vconf.usb.status"},
    // Power saving
    {"db/sysman/low_power_mode",
     EventType::kSystemSetting,
     "vconf.power.low_power_mode"},
    // Language & region
    {"db/menu_widget/language",
     EventType::kSystemSetting,
     "vconf.system.language"},
    {"db/menu_widget/regionformat",
     EventType::kSystemSetting,
     "vconf.system.region_format"},
    {"db/menu_widget/regionformat_time1224",
     EventType::kSystemSetting,
     "vconf.system.time_format_1224"},
    {"db/setting/region_automatic",
     EventType::kSystemSetting,
     "vconf.system.region_automatic"},
    // Timezone
    {"db/setting/timezone_id",
     EventType::kSystemSetting,
     "vconf.system.timezone_id"},
    {"db/setting/timezone",
     EventType::kSystemSetting,
     "vconf.system.timezone"},
    // Accessibility
    {"db/setting/accessibility/font_size",
     EventType::kSystemSetting,
     "vconf.accessibility.font_size"},
    // Lock screen
    {"db/setting/screen_lock_type",
     EventType::kSystemSetting,
     "vconf.system.screen_lock_type"},
};

const size_t VconfEventAdapter::kMappingCount =
    sizeof(kMappings) / sizeof(kMappings[0]);

VconfEventAdapter::~VconfEventAdapter() {
  Stop();
}

void VconfEventAdapter::Start() {
  if (started_) return;

  int success = 0;
  for (size_t i = 0; i < kMappingCount; ++i) {
    int ret = vconf_notify_key_changed(
        kMappings[i].key, OnVconfChanged, this);
    if (ret != VCONF_OK) {
      LOG(WARNING)
          << "VconfEventAdapter: failed to watch '"
          << kMappings[i].key
          << "', error=" << ret;
    } else {
      ++success;
    }
  }

  started_ = true;
  LOG(INFO) << "VconfEventAdapter: started with "
            << success << "/" << kMappingCount
            << " keys registered";
}

void VconfEventAdapter::Stop() {
  if (!started_) return;

  for (size_t i = 0; i < kMappingCount; ++i) {
    vconf_ignore_key_changed(
        kMappings[i].key, OnVconfChanged);
  }

  started_ = false;
  LOG(INFO) << "VconfEventAdapter: stopped";
}

std::string VconfEventAdapter::GetName() const {
  return "VconfEventAdapter";
}

nlohmann::json VconfEventAdapter::ExtractValue(
    keynode_t* node) {
  int type = vconf_keynode_get_type(node);
  switch (type) {
    case VCONF_TYPE_INT:
      return vconf_keynode_get_int(node);
    case VCONF_TYPE_BOOL:
      return static_cast<bool>(
          vconf_keynode_get_bool(node));
    case VCONF_TYPE_DOUBLE:
      return vconf_keynode_get_dbl(node);
    case VCONF_TYPE_STRING: {
      char* str = vconf_keynode_get_str(node);
      return str ? std::string(str) : "";
    }
    default:
      return nullptr;
  }
}

// Callback: invoked on the main thread's GLib
// Main Loop as an idle source. Must be non-blocking.
void VconfEventAdapter::OnVconfChanged(
    keynode_t* node, void* user_data) {
  if (!node || !user_data) return;

  char* key_name = vconf_keynode_get_name(node);
  if (!key_name) return;

  // Find the mapping for this key
  const KeyMapping* mapping = nullptr;
  for (size_t i = 0; i < kMappingCount; ++i) {
    if (std::strcmp(kMappings[i].key,
                   key_name) == 0) {
      mapping = &kMappings[i];
      break;
    }
  }

  if (!mapping) return;

  SystemEvent ev;
  ev.type = mapping->type;
  ev.source = "vconf";
  ev.name = mapping->event_name;
  ev.plugin_id = "builtin";
  ev.data["key"] = key_name;
  ev.data["value"] = ExtractValue(node);

  LOG(INFO) << "VconfEventAdapter: "
            << mapping->event_name
            << " key=" << key_name;

  // Non-blocking publish to EventBus dispatch
  // thread. Safe from main loop context.
  EventBus::GetInstance().Publish(std::move(ev));
}

}  // namespace tizenclaw
