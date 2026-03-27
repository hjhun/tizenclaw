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
#include "event_adapter_manager.hh"

#include "../../common/logging.hh"
#include "../infra/dbus_probe.hh"

namespace tizenclaw {

EventAdapterManager::~EventAdapterManager() {
  StopAll();
}

void EventAdapterManager::RegisterAdapter(
    std::unique_ptr<IEventAdapter> adapter) {
  if (!adapter) return;

  std::lock_guard<std::mutex> lock(mutex_);
  LOG(INFO) << "EventAdapterManager: registering '"
            << adapter->GetName() << "'";
  adapters_.push_back(std::move(adapter));
}

void EventAdapterManager::StartAll() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (started_) return;

  bool dbus_ok = DbusProbe::IsAvailable();
  if (!dbus_ok) {
    LOG(WARNING) << "EventAdapterManager: D-Bus "
                 << "unavailable, D-Bus dependent "
                 << "adapters will be skipped";
  }

  for (auto& adapter : adapters_) {
    if (!dbus_ok && adapter->UsesDBus()) {
      LOG(WARNING) << "EventAdapterManager: "
                   << "skipping '"
                   << adapter->GetName()
                   << "' (D-Bus unavailable)";
      continue;
    }
    LOG(INFO) << "EventAdapterManager: starting '"
              << adapter->GetName() << "'";
    try {
      adapter->Start();
    } catch (const std::exception& e) {
      LOG(ERROR) << "EventAdapterManager: '"
                 << adapter->GetName()
                 << "' failed to start: "
                 << e.what();
    }
  }
  started_ = true;
  LOG(INFO) << "EventAdapterManager: all "
            << adapters_.size()
            << " adapters processed";
}

void EventAdapterManager::StopAll() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!started_) return;

  for (auto& adapter : adapters_) {
    LOG(INFO) << "EventAdapterManager: stopping '"
              << adapter->GetName() << "'";
    adapter->Stop();
  }
  started_ = false;
  LOG(INFO) << "EventAdapterManager: all adapters "
            << "stopped";
}

std::vector<std::string>
EventAdapterManager::ListAdapters() const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<std::string> names;
  names.reserve(adapters_.size());
  for (const auto& adapter : adapters_) {
    names.push_back(adapter->GetName());
  }
  return names;
}

}  // namespace tizenclaw
