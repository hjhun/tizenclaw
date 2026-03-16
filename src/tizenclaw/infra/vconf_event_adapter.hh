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
#ifndef VCONF_EVENT_ADAPTER_HH
#define VCONF_EVENT_ADAPTER_HH

#include <vconf.h>

#include <string>
#include <vector>

#include "event_adapter.hh"
#include "event_bus.hh"

namespace tizenclaw {

// Wraps Tizen vconf API (buxton2/vconf-compat) to
// monitor system setting key changes in real-time.
//
// Threading model:
//   vconf callbacks are dispatched as GLib idle
//   sources on the main thread's GLib Main Loop.
//   The callback must remain non-blocking.
//   EventBus::Publish() is safe to call here.
class VconfEventAdapter : public IEventAdapter {
 public:
  VconfEventAdapter() = default;
  ~VconfEventAdapter() override;

  void Start() override;
  void Stop() override;
  [[nodiscard]] std::string GetName() const override;

 private:
  // Mapping from vconf key to EventBus event
  struct KeyMapping {
    const char* key;
    EventType type;
    const char* event_name;
  };

  // Static callback for vconf_notify_key_changed.
  // Invoked on the main thread (GLib idle source).
  static void OnVconfChanged(
      keynode_t* node, void* user_data);

  // Extract typed value from keynode_t into JSON
  static nlohmann::json ExtractValue(
      keynode_t* node);

  // Built-in key mapping table
  static const KeyMapping kMappings[];
  static const size_t kMappingCount;

  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // VCONF_EVENT_ADAPTER_HH
