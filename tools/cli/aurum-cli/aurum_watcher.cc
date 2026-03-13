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

#include "aurum_watcher.hh"

#include <tizen_core.h>

#include <atomic>
#include <chrono>
#include <csignal>
#include <iostream>
#include <thread>

namespace {

std::atomic<bool> g_running{true};

void SignalHandler(int) {
  g_running.store(false);
}

struct WatcherContext {
  std::function<void(const std::string&)> on_event;
};

// EventHandler signature:
//   bool(void*, const A11yEvent,
//        std::shared_ptr<AccessibleNode>)
bool OnA11yEvent(
    void* data,
    const Aurum::A11yEvent /*event*/,
    std::shared_ptr<Aurum::AccessibleNode> /*node*/) {
  auto* ctx = static_cast<WatcherContext*>(data);
  if (ctx && ctx->on_event)
    ctx->on_event("event_received");
  return true;  // keep listening
}

}  // namespace

namespace aurum_cli {

bool RunWatcher(
    Aurum::A11yEvent event_type,
    int timeout_ms,
    const std::function<void(const std::string&)>&
        on_event) {
  std::signal(SIGINT, SignalHandler);
  std::signal(SIGTERM, SignalHandler);

  auto device = Aurum::UiDevice::getInstance();
  if (!device) {
    std::cerr << "Failed to get UiDevice\n";
    return false;
  }

  WatcherContext ctx{on_event};
  if (!device->registerCallback(
          event_type, OnA11yEvent, &ctx)) {
    std::cerr << "Failed to register callback\n";
    return false;
  }

  // Poll-based timeout loop
  int elapsed_ms = 0;
  constexpr int kPollMs = 100;

  while (g_running.load() &&
         (timeout_ms <= 0 ||
          elapsed_ms < timeout_ms)) {
    std::this_thread::sleep_for(
        std::chrono::milliseconds(kPollMs));
    elapsed_ms += kPollMs;
  }

  device->clearCallback();
  return true;
}

}  // namespace aurum_cli
