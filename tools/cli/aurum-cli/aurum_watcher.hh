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

#ifndef AURUM_WATCHER_HH_
#define AURUM_WATCHER_HH_

#include <Aurum.h>

#include <functional>
#include <string>

namespace aurum_cli {

// Runs a tizen-core event loop that registers an
// AT-SPI2 callback via UiDevice::registerCallback().
// Blocks until timeout expires or SIGINT/SIGTERM.
// Calls on_event for each received event.
bool RunWatcher(
    Aurum::A11yEvent event_type,
    int timeout_ms,
    const std::function<void(const std::string&)>& on_event);

}  // namespace aurum_cli

#endif  // AURUM_WATCHER_HH_
