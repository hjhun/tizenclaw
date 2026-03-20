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
#ifndef BOOT_STATUS_LOGGER_HH
#define BOOT_STATUS_LOGGER_HH

#include <chrono>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include "file_log_backend.hh"

namespace tizenclaw {

// Systemd-style boot status logger.
//
// Records module initialization status in a human-readable format
// similar to systemd's boot output:
//
//   [  OK  ] Starting PluginManager...
//   [  OK  ] Started PluginManager (12ms)
//   [FAILED] Started ContainerEngine: timeout
//
// The log is written to a dedicated file backend
// (path configurable via Initialize()) to provide
// a clear overview of the daemon's boot health.
class BootStatusLogger {
 public:
  static BootStatusLogger& GetInstance();

  // Initialize the logger with the given log file path.
  // Must be called once before any Starting/Started/Failed calls.
  void Initialize(const std::string& log_path);

  // Record that a module is beginning initialization.
  // Prints: "[  OK  ] Starting <module>..."
  void Starting(const std::string& module);

  // Record that a module has completed initialization.
  // Prints: "[  OK  ] Started <module> (<elapsed>ms)"
  void Started(const std::string& module);

  // Record that a module failed to initialize.
  // Prints: "[FAILED] <phase> <module>: <reason>"
  //   phase is "Starting" if called before Started(),
  //   or "Started" otherwise.
  void Failed(const std::string& module,
              const std::string& reason = "");

  // RAII helper for auto-tracking starting/started/failed.
  // Usage:
  //   {
  //     auto guard = BootStatusLogger::GetInstance()
  //         .Track("ModuleName");
  //     bool ok = module.Initialize();
  //     if (!ok) guard.SetFailed("init error");
  //   }  // Automatically logs Started or Failed on destruction.
  class TrackGuard {
   public:
    TrackGuard(BootStatusLogger* logger,
               std::string module);
    ~TrackGuard();

    TrackGuard(const TrackGuard&) = delete;
    TrackGuard& operator=(const TrackGuard&) = delete;
    TrackGuard(TrackGuard&& other) noexcept;
    TrackGuard& operator=(TrackGuard&& other) noexcept;

    // Mark this module's initialization as failed.
    void SetFailed(const std::string& reason = "");

   private:
    BootStatusLogger* logger_ = nullptr;
    std::string module_;
    bool failed_ = false;
    std::string fail_reason_;
  };

  // Create a RAII track guard for automatic status logging.
  [[nodiscard]] TrackGuard Track(const std::string& module);

  // Print a summary line after boot completes.
  // e.g. "Boot complete: 18 OK, 1 FAILED (1234ms)"
  void PrintSummary();

 private:
  BootStatusLogger() = default;
  ~BootStatusLogger() = default;
  BootStatusLogger(const BootStatusLogger&) = delete;
  BootStatusLogger& operator=(const BootStatusLogger&) = delete;

  void WriteStatus(const std::string& status,
                   const std::string& message);

  std::shared_ptr<utils::FileLogBackend> backend_;
  std::mutex mutex_;
  bool initialized_ = false;

  // Per-module timing
  struct ModuleTimer {
    std::string name;
    std::chrono::steady_clock::time_point start;
  };
  std::vector<ModuleTimer> timers_;

  // Boot-level timing
  std::chrono::steady_clock::time_point boot_start_;

  // Counters
  int ok_count_ = 0;
  int fail_count_ = 0;
};

}  // namespace tizenclaw

#endif  // BOOT_STATUS_LOGGER_HH
