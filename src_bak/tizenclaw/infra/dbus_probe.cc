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
#include "dbus_probe.hh"

#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#include <atomic>

#include "../../common/logging.hh"

namespace {

enum class ProbeState {
  kUntested = 0,
  kAvailable = 1,
  kUnavailable = 2,
};

std::atomic<ProbeState> g_state{ProbeState::kUntested};

// The child process attempts to open the D-Bus
// system bus socket.  If libdbuspolicy1 triggers
// assert(false) inside udesc::init_once(), the
// child will be killed by SIGABRT.
// We intentionally avoid linking libdbus here;
// instead we check connectivity by trying to open
// the well-known system bus socket path.
bool RunProbe() {
  pid_t pid = fork();
  if (pid < 0) {
    // fork failed — assume D-Bus is available
    // so we don't silently disable features.
    return true;
  }

  if (pid == 0) {
    // Child: attempt to connect to the D-Bus
    // system bus socket.  The glib/dbus libraries
    // loaded via Tizen APIs will trigger
    // libdbuspolicy1 init on first use.
    // We use a lightweight approach: just check
    // the existence and accessibility of the
    // D-Bus system bus socket.
    const char* bus_addr =
        "/run/dbus/system_bus_socket";
    if (access(bus_addr, R_OK | W_OK) == 0) {
      _exit(0);  // socket accessible
    }
    _exit(1);  // socket not accessible
  }

  // Parent: wait for child result.
  int status = 0;
  if (waitpid(pid, &status, 0) < 0) {
    return true;  // waitpid error — assume ok
  }

  if (WIFSIGNALED(status)) {
    int sig = WTERMSIG(status);
    LOG(WARNING) << "DbusProbe: child killed by "
                 << "signal " << sig
                 << (sig == 6 ? " (SIGABRT)" : "");
    return false;
  }

  if (WIFEXITED(status)) {
    int code = WEXITSTATUS(status);
    if (code != 0) {
      LOG(WARNING) << "DbusProbe: D-Bus socket "
                   << "not accessible (exit="
                   << code << ")";
      return false;
    }
    return true;
  }

  return true;
}

}  // namespace

namespace tizenclaw {

bool DbusProbe::IsAvailable() {
  ProbeState expected = ProbeState::kUntested;
  if (g_state.compare_exchange_strong(
          expected, ProbeState::kAvailable)) {
    // First caller — run the probe.
    bool ok = RunProbe();
    g_state.store(ok ? ProbeState::kAvailable
                     : ProbeState::kUnavailable);
    LOG(INFO) << "DbusProbe: D-Bus system bus "
              << (ok ? "available" : "unavailable");
    return ok;
  }

  // Wait for probe to finish if another thread
  // is running it concurrently (rare).
  while (g_state.load() == ProbeState::kUntested) {
    usleep(1000);
  }

  return g_state.load() == ProbeState::kAvailable;
}

}  // namespace tizenclaw
