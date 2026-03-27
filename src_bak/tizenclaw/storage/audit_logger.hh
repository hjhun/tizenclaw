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
#ifndef AUDIT_LOGGER_HH
#define AUDIT_LOGGER_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

enum class AuditEventType {
  kIpcConnect,     // Client connected
  kIpcAuth,        // Auth success/fail
  kToolExecution,  // Tool executed
  kToolBlocked,    // Tool blocked by policy
  kSessionCreate,  // New session
  kConfigChange,   // Config loaded/changed
};

struct AuditEvent {
  std::string timestamp;
  AuditEventType type;
  std::string session_id;
  nlohmann::json details;
};

class AuditLogger {
 public:
  static AuditLogger& Instance();

  void SetLogDir(const std::string& dir);
  void Log(const AuditEvent& event);

  // Query audit entries from a date's file
  std::vector<AuditEvent> Query(const std::string& date,
                                const std::string& type_filter = "");

  // Convenience helpers
  static AuditEvent MakeEvent(AuditEventType type,
                              const std::string& session_id = "",
                              const nlohmann::json& details = {});

  static std::string TypeToString(AuditEventType type);

 private:
  AuditLogger();
  AuditLogger(const AuditLogger&) = delete;
  AuditLogger& operator=(const AuditLogger&) = delete;

  // Format event as Markdown table row
  std::string EventToRow(const AuditEvent& event) const;

  // Build details string from JSON
  std::string DetailsToString(const nlohmann::json& details) const;

  // Get current time as HH:MM:SS
  static std::string GetTimeStr();

  // Get current date as YYYY-MM-DD
  static std::string GetDateStr();

  // Get ISO timestamp
  static std::string GetTimestamp();

  // Ensure directory exists
  static void EnsureDir(const std::string& dir);

  // Rotate file if too large
  void RotateIfNeeded(const std::string& path);

  std::string log_dir_;
  std::mutex mutex_;

  static constexpr size_t kMaxFileSize = 5 * 1024 * 1024;  // 5MB
  static constexpr int kMaxRotation = 5;
};

}  // namespace tizenclaw

#endif  // AUDIT_LOGGER_HH
