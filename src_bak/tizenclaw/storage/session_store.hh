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
#ifndef SESSION_STORE_HH
#define SESSION_STORE_HH

#include <json.hpp>
#include <string>
#include <vector>

#include "../llm/llm_backend.hh"

namespace tizenclaw {

// Skill execution log entry
struct SkillLogEntry {
  std::string timestamp;
  std::string session_id;
  std::string skill_name;
  nlohmann::json args;
  std::string result;
  int duration_ms = 0;
};

// Token usage entry
struct TokenUsageEntry {
  std::string timestamp;
  std::string model_name;
  int prompt_tokens = 0;
  int completion_tokens = 0;
};

// Aggregated token usage summary
struct TokenUsageSummary {
  int total_prompt_tokens = 0;
  int total_completion_tokens = 0;
  std::vector<TokenUsageEntry> entries;
};

// Daily/Monthly aggregated usage summary
struct DailyUsageSummary {
  std::string date;  // YYYY-MM-DD or YYYY-MM
  int total_prompt_tokens = 0;
  int total_completion_tokens = 0;
  int total_requests = 0;
  struct BackendEntry {
    std::string backend;
    int requests = 0;
    int prompt_tokens = 0;
    int completion_tokens = 0;
  };
  std::vector<BackendEntry> backends;
};

class SessionStore {
 public:
  SessionStore();

  // Set the directory for session files
  void SetDirectory(const std::string& dir);

  // Save session history to disk (Markdown format)
  [[nodiscard]] bool SaveSession(const std::string& session_id,
                                 const std::vector<LlmMessage>& history);

  // Load session history from disk (Markdown or
  // legacy JSON with auto-migration)
  [[nodiscard]] std::vector<LlmMessage> LoadSession(
      const std::string& session_id);

  // Delete a session file
  void DeleteSession(const std::string& session_id);

  // Skill execution logging
  void LogSkillExecution(const std::string& session_id,
                         const std::string& skill_name,
                         const nlohmann::json& args, const std::string& result,
                         int duration_ms);

  // Token usage logging
  void LogTokenUsage(const std::string& session_id,
                     const std::string& model_name, int prompt_tokens,
                     int completion_tokens);

  // Load token usage for a session
  [[nodiscard]] TokenUsageSummary LoadTokenUsage(const std::string& session_id) const;

  // Load daily aggregate usage
  [[nodiscard]] DailyUsageSummary LoadDailyUsage(const std::string& date) const;

  // Load monthly aggregate usage
  [[nodiscard]] DailyUsageSummary LoadMonthlyUsage(
      const std::string& month) const;

  // Remove orphaned tool messages that have no
  // matching tool_calls in a preceding assistant
  // message. This prevents OpenAI HTTP 400 errors.
  static void SanitizeHistory(std::vector<LlmMessage>& history);

 private:
  // Find existing session file by scanning
  // dir for *-{session_id}.md pattern
  std::string FindSessionFile(const std::string& dir,
                              const std::string& session_id) const;

  std::string GetSessionPath(const std::string& session_id) const;
  std::string GetLegacySessionPath(const std::string& session_id) const;
  std::string GetLogsDir() const;
  std::string GetUsageDir() const;
  std::string GetDailyUsageDir() const;
  std::string GetMonthlyUsageDir() const;

  // Get current date as YYYY-MM-DD string
  static std::string GetDatePrefix();

  // Markdown serialization
  std::string MessagesToMarkdown(const std::vector<LlmMessage>& history) const;
  std::vector<LlmMessage> MarkdownToMessages(const std::string& content) const;

  // Legacy JSON support
  static nlohmann::json MessageToJson(const LlmMessage& msg);
  static LlmMessage JsonToMessage(const nlohmann::json& j);

  // Get current timestamp as ISO string
  static std::string GetTimestamp();

  // Ensure a directory exists (mkdir -p)
  static void EnsureDir(const std::string& dir);

  // Atomic file write (write .tmp then rename)
  static bool AtomicWrite(const std::string& path, const std::string& content);

  std::string sessions_dir_;

  // Max file size in bytes (512KB)
  static constexpr size_t kMaxFileSize = 512 * 1024;
};

}  // namespace tizenclaw

#endif  // SESSION_STORE_HH
