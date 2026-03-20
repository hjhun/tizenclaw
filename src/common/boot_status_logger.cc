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
#include "boot_status_logger.hh"

#include <algorithm>
#include <filesystem>
#include <iomanip>
#include <sstream>

#include "logging.hh"

namespace tizenclaw {

namespace {

constexpr const char* kStatusOk = "[  OK  ]";
constexpr const char* kStatusFailed = "[FAILED]";

}  // namespace

BootStatusLogger& BootStatusLogger::GetInstance() {
  static BootStatusLogger instance;
  return instance;
}

void BootStatusLogger::Initialize(
    const std::string& log_path) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (initialized_) return;

  // Ensure log directory exists
  namespace fs = std::filesystem;
  fs::path dir = fs::path(log_path).parent_path();
  std::error_code ec;
  if (!dir.empty() && !fs::exists(dir, ec))
    fs::create_directories(dir, ec);

  // 512KB rotation, keep 3 backups
  backend_ =
      std::make_shared<utils::FileLogBackend>(
          log_path, 512 * 1024, 3);

  boot_start_ =
      std::chrono::steady_clock::now();
  ok_count_ = 0;
  fail_count_ = 0;
  timers_.clear();
  initialized_ = true;

  // Write boot header
  WriteStatus("------",
      "TizenClaw Boot Sequence");
}

void BootStatusLogger::Starting(
    const std::string& module) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!initialized_) return;

  // Record start time
  timers_.push_back(
      {module,
       std::chrono::steady_clock::now()});

  std::string msg =
      "Starting " + module + "...";
  WriteStatus(kStatusOk, msg);
}

void BootStatusLogger::Started(
    const std::string& module) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!initialized_) return;

  // Calculate elapsed time
  std::string elapsed_str;
  auto it = std::ranges::find_if(
      timers_,
      [&module](const ModuleTimer& t) {
        return t.name == module;
      });
  if (it != timers_.end()) {
    auto elapsed =
        std::chrono::duration_cast<
            std::chrono::milliseconds>(
            std::chrono::steady_clock::now() -
            it->start)
            .count();
    elapsed_str =
        " (" + std::to_string(elapsed) + "ms)";
    timers_.erase(it);
  }

  ++ok_count_;
  std::string msg =
      "Started " + module + elapsed_str;
  WriteStatus(kStatusOk, msg);
}

void BootStatusLogger::Failed(
    const std::string& module,
    const std::string& reason) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!initialized_) return;

  // Determine phase based on whether Starting
  // was called (timer exists)
  std::string phase = "Started";
  auto it = std::ranges::find_if(
      timers_,
      [&module](const ModuleTimer& t) {
        return t.name == module;
      });
  if (it != timers_.end()) {
    phase = "Starting";
    timers_.erase(it);
  }

  ++fail_count_;
  std::string msg = phase + " " + module;
  if (!reason.empty())
    msg += ": " + reason;

  WriteStatus(kStatusFailed, msg);
}

BootStatusLogger::TrackGuard::TrackGuard(
    BootStatusLogger* logger,
    std::string module)
    : logger_(logger),
      module_(std::move(module)) {
  if (logger_)
    logger_->Starting(module_);
}

BootStatusLogger::TrackGuard::~TrackGuard() {
  if (!logger_) return;
  if (failed_)
    logger_->Failed(module_, fail_reason_);
  else
    logger_->Started(module_);
}

BootStatusLogger::TrackGuard::TrackGuard(
    TrackGuard&& other) noexcept
    : logger_(other.logger_),
      module_(std::move(other.module_)),
      failed_(other.failed_),
      fail_reason_(
          std::move(other.fail_reason_)) {
  other.logger_ = nullptr;
}

BootStatusLogger::TrackGuard&
BootStatusLogger::TrackGuard::operator=(
    TrackGuard&& other) noexcept {
  if (this != &other) {
    logger_ = other.logger_;
    module_ = std::move(other.module_);
    failed_ = other.failed_;
    fail_reason_ =
        std::move(other.fail_reason_);
    other.logger_ = nullptr;
  }
  return *this;
}

void BootStatusLogger::TrackGuard::SetFailed(
    const std::string& reason) {
  failed_ = true;
  fail_reason_ = reason;
}

BootStatusLogger::TrackGuard
BootStatusLogger::Track(
    const std::string& module) {
  return TrackGuard(this, module);
}

void BootStatusLogger::PrintSummary() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!initialized_) return;

  auto total_elapsed =
      std::chrono::duration_cast<
          std::chrono::milliseconds>(
          std::chrono::steady_clock::now() -
          boot_start_)
          .count();

  std::ostringstream oss;
  oss << "Boot complete: "
      << ok_count_ << " OK, "
      << fail_count_ << " FAILED"
      << " (" << total_elapsed << "ms)";

  WriteStatus("------", oss.str());
}

void BootStatusLogger::WriteStatus(
    const std::string& status,
    const std::string& message) {
  if (!backend_) return;

  std::string line = status + " " + message;

  // Write to file backend (level INFO,
  // tag BOOT)
  backend_->WriteLog(
      utils::LogLevel::LOG_INFO,
      "BOOT", line);

  // Also emit to dlog for real-time visibility
  LOG(INFO) << "[BOOT] " << line;
}

}  // namespace tizenclaw
