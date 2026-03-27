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
#include "audit_logger.hh"

#include <sys/stat.h>

#include <chrono>
#include <ctime>
#include <fstream>
#include <iomanip>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

AuditLogger::AuditLogger() : log_dir_("/opt/usr/share/tizenclaw/audit") {}

AuditLogger& AuditLogger::Instance() {
  static AuditLogger instance;
  return instance;
}

void AuditLogger::SetLogDir(const std::string& dir) {
  std::lock_guard<std::mutex> lock(mutex_);
  log_dir_ = dir;
}

void AuditLogger::Log(const AuditEvent& event) {
  std::lock_guard<std::mutex> lock(mutex_);

  EnsureDir(log_dir_);

  std::string date = GetDateStr();
  std::string path = log_dir_ + "/" + date + ".md";

  RotateIfNeeded(path);

  // Check if file exists to determine if we
  // need the header
  bool needs_header = true;
  {
    std::ifstream check(path);
    needs_header =
        !check.good() || check.peek() == std::ifstream::traits_type ::eof();
  }

  std::ofstream f(path, std::ios::app);
  if (!f.is_open()) {
    LOG(ERROR) << "Cannot open audit log: " << path;
    return;
  }

  if (needs_header) {
    f << "---\n"
      << "date: " << date << "\n"
      << "type: audit_log\n"
      << "---\n\n"
      << "## Audit Events\n\n"
      << "| Time | Type | Session " << "| Details |\n"
      << "|------|------|--------- " << "|---------|\n";
  }

  f << EventToRow(event) << "\n";
  f.close();

  // Also log via dlog for backward compat
  LOG(INFO) << "[AUDIT] " << TypeToString(event.type) << " session="
            << (event.session_id.empty() ? "-" : event.session_id) << " "
            << DetailsToString(event.details);
}

AuditEvent AuditLogger::MakeEvent(AuditEventType type,
                                  const std::string& session_id,
                                  const nlohmann::json& details) {
  AuditEvent event;
  event.timestamp = GetTimestamp();
  event.type = type;
  event.session_id = session_id;
  event.details = details;
  return event;
}

std::vector<AuditEvent> AuditLogger::Query(const std::string& date,
                                           const std::string& type_filter) {
  std::vector<AuditEvent> results;
  std::string path = log_dir_ + "/" + date + ".md";

  std::ifstream f(path);
  if (!f.is_open()) return results;

  std::string line;
  bool in_table = false;

  while (std::getline(f, line)) {
    // Skip headers and separator
    if (line.find("| Time") != std::string::npos ||
        line.find("|---") != std::string::npos) {
      in_table = true;
      continue;
    }

    if (!in_table || line.empty() || line[0] != '|') {
      continue;
    }

    // Parse: | time | type | session | details|
    std::vector<std::string> cols;
    std::istringstream ss(line);
    std::string col;
    while (std::getline(ss, col, '|')) {
      // Trim whitespace
      size_t start = col.find_first_not_of(' ');
      size_t end = col.find_last_not_of(' ');
      if (start != std::string::npos) {
        cols.push_back(col.substr(start, end - start + 1));
      }
    }

    if (cols.size() >= 4) {
      AuditEvent event;
      event.timestamp = cols[0];
      // cols[1] is type string
      event.session_id = cols[2];
      event.details = {{"raw", cols[3]}};

      // Apply type filter
      if (!type_filter.empty() && cols[1] != type_filter) {
        continue;
      }

      results.push_back(event);
    }
  }

  return results;
}

std::string AuditLogger::EventToRow(const AuditEvent& event) const {
  std::string session = event.session_id.empty() ? "-" : event.session_id;

  return "| " + GetTimeStr() + " | " + TypeToString(event.type) + " | " +
         session + " | " + DetailsToString(event.details) + " |";
}

std::string AuditLogger::DetailsToString(const nlohmann::json& details) const {
  if (details.is_null() || details.empty()) {
    return "-";
  }

  std::string result;
  for (auto& [key, val] : details.items()) {
    if (!result.empty()) result += ", ";
    result += key + "=";
    if (val.is_string()) {
      result += val.get<std::string>();
    } else {
      result += val.dump();
    }
  }

  return result;
}

std::string AuditLogger::TypeToString(AuditEventType type) {
  switch (type) {
    case AuditEventType::kIpcConnect:
      return "ipc_connect";
    case AuditEventType::kIpcAuth:
      return "ipc_auth";
    case AuditEventType::kToolExecution:
      return "tool_execution";
    case AuditEventType::kToolBlocked:
      return "tool_blocked";
    case AuditEventType::kSessionCreate:
      return "session_create";
    case AuditEventType::kConfigChange:
      return "config_change";
    default:
      return "unknown";
  }
}

std::string AuditLogger::GetTimeStr() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  struct tm tm_buf;
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%H:%M:%S");
  return oss.str();
}

std::string AuditLogger::GetDateStr() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  struct tm tm_buf;
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%d");
  return oss.str();
}

std::string AuditLogger::GetTimestamp() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  struct tm tm_buf;
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%dT%H:%M:%S");
  return oss.str();
}

void AuditLogger::EnsureDir(const std::string& dir) {
  struct stat st;
  if (stat(dir.c_str(), &st) != 0) {
    mkdir(dir.c_str(), 0755);
  }
}

void AuditLogger::RotateIfNeeded(const std::string& path) {
  struct stat st;
  if (stat(path.c_str(), &st) != 0) return;

  if (static_cast<size_t>(st.st_size) < kMaxFileSize) {
    return;
  }

  // Rotate: file.md → file.1.md, etc.
  for (int i = kMaxRotation - 1; i >= 1; --i) {
    std::string from = path + "." + std::to_string(i);
    std::string to = path + "." + std::to_string(i + 1);
    rename(from.c_str(), to.c_str());
  }
  rename(path.c_str(), (path + ".1").c_str());
}

}  // namespace tizenclaw
