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
#ifndef DBUS_PROBE_HH
#define DBUS_PROBE_HH

namespace tizenclaw {

// Fork-based D-Bus availability probe.
// Spawns a short-lived child process that attempts
// a D-Bus system bus connection.  If the child is
// killed by SIGABRT (e.g.  libdbuspolicy1 assertion)
// the probe reports D-Bus as unavailable.
// The result is cached after the first call.
class DbusProbe {
 public:
  // Returns true if D-Bus system bus is reachable
  // without triggering assertion crashes.
  // Thread-safe; result is cached after first call.
  static bool IsAvailable();

 private:
  DbusProbe() = delete;
};

}  // namespace tizenclaw

#endif  // DBUS_PROBE_HH
