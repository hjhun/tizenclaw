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
#include "file_log_backend.hh"

#include <unistd.h>

#include <chrono>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <mutex>
#include <sstream>
#include <string>
#include <utility>

namespace tizenclaw {

namespace fs = std::filesystem;

namespace utils {

FileLogBackend::FileLogBackend(std::string file_path, int rotation_size,
                               int max_rotation)
    : file_path_(std::move(file_path)),
      rotation_size_(rotation_size),
      max_rotation_(max_rotation) {}

void FileLogBackend::WriteLog(LogLevel level, const std::string& /* tag */,
                              const std::string& logstr) {
  if (level == LogLevel::LOG_DEBUG) return;

  std::lock_guard<std::mutex> lock(mutex_);

  if (file_path_.empty()) return;

  // Rotate check
  std::error_code ec;
  if (fs::exists(file_path_, ec) &&
      fs::file_size(file_path_, ec) > static_cast<uintmax_t>(rotation_size_)) {
    Rotate();
  }

  // Write
  std::ofstream ofs(file_path_, std::ios::app);
  if (ofs.is_open()) {
    ofs << GetTimeStamp() << GetPid() << logstr << std::endl;

    // Set permissions: Owner RW, Group R (640)
    try {
      fs::permissions(file_path_,
                      fs::perms::owner_read | fs::perms::owner_write |
                          fs::perms::group_read,
                      fs::perm_options::replace, ec);
    } catch (...) {
    }
  }
}

bool FileLogBackend::Rotate() {
  try {
    fs::path base_path(file_path_);

    // Rotate existing backups
    for (int i = max_rotation_; i > 0; --i) {
      fs::path src = base_path;
      src += "." + std::to_string(i);

      if (i == max_rotation_) {
        std::error_code ec;
        if (fs::exists(src, ec)) fs::remove(src, ec);
      } else {
        fs::path dest = base_path;
        dest += "." + std::to_string(i + 1);
        std::error_code ec;
        if (fs::exists(src, ec)) fs::rename(src, dest, ec);
      }
    }

    // Rotate current log to .1
    std::error_code ec;
    if (fs::exists(base_path, ec)) {
      fs::path dest = base_path;
      dest += ".1";
      fs::rename(base_path, dest, ec);
    }

    return true;
  } catch (...) {
    return false;
  }
}

int FileLogBackend::GetFileSize(const std::string& file_name) {
  std::error_code ec;
  if (fs::exists(file_name, ec)) {
    return static_cast<int>(fs::file_size(file_name, ec));
  }
  return -1;
}

std::string FileLogBackend::GetTimeStamp() {
  using namespace std::chrono;
  auto now = system_clock::now();
  auto tt = system_clock::to_time_t(now);
  auto ms = duration_cast<milliseconds>(now.time_since_epoch()) % 1000;

  std::tm tm_buf;
  gmtime_r(&tt, &tm_buf);

  std::stringstream ss;
  ss << std::put_time(&tm_buf, "%Y%m%d.%H%M%S");
  ss << "." << std::setfill('0') << std::setw(3) << ms.count() << "UTC|";
  return ss.str();
}

std::string FileLogBackend::GetPid() { return std::to_string(getpid()) + "|"; }

std::string FileLogBackend::GetLogDir() {
  return file_path_.substr(0, file_path_.find_last_of("\\/") + 1);
}

std::string FileLogBackend::GetFileName() {
  return file_path_.substr(file_path_.find_last_of("\\/") + 1);
}

}  // namespace utils

}  // namespace tizenclaw
