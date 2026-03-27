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
#ifndef EVENT_ADAPTER_MANAGER_HH
#define EVENT_ADAPTER_MANAGER_HH

#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include "event_adapter.hh"

namespace tizenclaw {

// Manages all registered IEventAdapter instances.
// Provides centralized Start/Stop and listing.
class EventAdapterManager {
 public:
  EventAdapterManager() = default;
  ~EventAdapterManager();

  // Register an adapter. Ownership is transferred.
  void RegisterAdapter(
      std::unique_ptr<IEventAdapter> adapter);

  // Start all registered adapters.
  void StartAll();

  // Stop all registered adapters.
  void StopAll();

  // List names of registered adapters.
  [[nodiscard]] std::vector<std::string>
  ListAdapters() const;

 private:
  mutable std::mutex mutex_;
  std::vector<std::unique_ptr<IEventAdapter>>
      adapters_;
  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // EVENT_ADAPTER_MANAGER_HH
