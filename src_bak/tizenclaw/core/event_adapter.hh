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
#ifndef EVENT_ADAPTER_HH
#define EVENT_ADAPTER_HH

#include <string>

namespace tizenclaw {

// Common interface for all event source adapters.
// Each adapter wraps a Tizen native C-API and
// publishes SystemEvent objects to the EventBus.
class IEventAdapter {
 public:
  virtual ~IEventAdapter() = default;

  // Start monitoring events from this source.
  virtual void Start() = 0;

  // Stop monitoring and release resources.
  virtual void Stop() = 0;

  // Human-readable name for this adapter.
  [[nodiscard]] virtual std::string GetName() const = 0;

  // Whether this adapter depends on D-Bus IPC.
  // Adapters that return true will be skipped when
  // the D-Bus system bus is not reachable.
  [[nodiscard]] virtual bool UsesDBus() const {
    return false;
  }
};

}  // namespace tizenclaw

#endif  // EVENT_ADAPTER_HH
