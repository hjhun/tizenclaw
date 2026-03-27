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
#ifndef TIZEN_SYSTEM_EVENT_ADAPTER_HH
#define TIZEN_SYSTEM_EVENT_ADAPTER_HH

#include <app_event.h>

#include <string>
#include <vector>

#include "event_adapter.hh"

namespace tizenclaw {

// Wraps Tizen app_event.h System Event API.
// Monitors battery, network, display, bluetooth,
// USB, location, memory, and system settings.
class TizenSystemEventAdapter : public IEventAdapter {
 public:
  TizenSystemEventAdapter() = default;
  ~TizenSystemEventAdapter() override;

  void Start() override;
  void Stop() override;
  [[nodiscard]] std::string GetName() const override;

 private:
  // Internal event handler registration
  void RegisterSystemEvent(
      const char* event_name);

  // Static callback dispatched by app_event API
  static void OnSystemEvent(
      const char* event_name,
      bundle* event_data,
      void* user_data);

  std::vector<event_handler_h> handlers_;
  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // TIZEN_SYSTEM_EVENT_ADAPTER_HH
