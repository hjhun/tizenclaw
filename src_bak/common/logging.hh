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
#ifndef LOGGING_HH
#define LOGGING_HH

#include <dlog.h>

#include <cassert>
#include <cstring>
#include <iomanip>
#include <iostream>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <vector>

namespace tizenclaw {

#ifndef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW"
#endif

#ifdef LOG
#undef LOG
#endif

#ifndef __FILENAME__
#define __FILENAME__ \
  (strrchr(__FILE__, '/') ? strrchr(__FILE__, '/') + 1 : __FILE__)
#endif

namespace utils {

enum class LogLevel {
  LOG_ERROR,
  LOG_WARNING,
  LOG_INFO,
  LOG_DEBUG,
};

[[nodiscard]] log_priority LogLevelToPriority(LogLevel level);

template <class charT, class traits = std::char_traits<charT>>
class StringStream : private std::basic_ostringstream<charT, traits> {
 public:
  using std::basic_ostringstream<charT, traits>::str;

  template <class T>
  StringStream& operator<<(const T& value) {
    static_cast<std::basic_ostringstream<charT, traits>&>(*this) << value;
    return *this;
  }
};

// Interface class for logging backends. The custom LogBackend which wants
// log using LOG() macro should be implement following interface.
class ILogBackend {
 public:
  virtual ~ILogBackend() = default;
  virtual void WriteLog(LogLevel level, const std::string& tag,
                        const std::string& logstr) = 0;
};

class LogCore {
 public:
  // Do not call this function at destructor of global object
  [[nodiscard]] static LogCore& GetCore() {
    static LogCore core;
    return core;
  }

  void AddLogBackend(std::shared_ptr<ILogBackend> backend) {
    std::lock_guard<std::mutex> lock(mutex_);
    backend_list_.emplace_back(backend);
  }

  void Log(LogLevel level, const std::string& tag, const std::string& log) {
    std::lock_guard<std::mutex> lock(mutex_);
    for (auto& backend : backend_list_) backend->WriteLog(level, tag, log);
  }

 private:
  LogCore() = default;
  ~LogCore() = default;
  LogCore(const LogCore&) = delete;
  LogCore& operator=(const LogCore&) = delete;

  std::vector<std::shared_ptr<ILogBackend>> backend_list_;
  std::mutex mutex_;
};

class LogCatcher {
 public:
  LogCatcher(LogLevel level, const char* tag) : level_(level), tag_(tag) {}

  void operator&(const StringStream<char>& str) const {
    // Direct dlog_print — proven to work in tizen-action
    dlog_print(LogLevelToPriority(level_), tag_.c_str(), "%s",
               Escape(str.str()).c_str());

    if (level_ == LogLevel::LOG_ERROR) std::cerr << str.str() << std::endl;

    // Dispatch to additional backends (e.g., FileLogBackend)
    LogCore::GetCore().Log(level_, tag_, str.str());
  }

 private:
  // Since LogCatcher passes input to dlog_print(), the input which contains
  // format string(such as %d, %n) can cause unexpected result.
  // This is simple function to escape '%'.
  // NOTE: Is there any gorgeous way instead of this?
  static std::string Escape(const std::string& str) {
    std::string escaped = std::string(str);
    size_t start_pos = 0;
    std::string from = "%";
    std::string to = "%%";
    while ((start_pos = escaped.find(from, start_pos)) != std::string::npos) {
      escaped.replace(start_pos, from.length(), to);
      start_pos += to.length();
    }
    return escaped;
  }

  LogLevel level_;
  std::string tag_;
};

}  // namespace utils

inline static consteval const char* __tag_for_project() { return PROJECT_TAG; }

// Simple logging macro of following usage:
//   LOG(LEVEL) << object_1 << object_2 << object_n;
//     where:
//       LEVEL = ERROR | WARNING | INFO | DEBUG
#define LOG(LEVEL)                                                           \
  ::tizenclaw::utils::LogCatcher(::tizenclaw::utils::LogLevel::LOG_##LEVEL,  \
                                 ::tizenclaw::__tag_for_project()) &         \
      ::tizenclaw::utils::StringStream<char>()                               \
          << std::setw(50) << std::right                                     \
          << (std::string(__FILENAME__) + ": " + std::string(__FUNCTION__) + \
              "(" + std::to_string(__LINE__) + ")")                          \
                 .c_str()                                                    \
          << std::setw(0) << " : "

}  // namespace tizenclaw

#endif  // LOGGING_HH
