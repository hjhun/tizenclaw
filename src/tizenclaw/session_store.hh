#ifndef __SESSION_STORE_H__
#define __SESSION_STORE_H__

#include <string>
#include <vector>
#include <json.hpp>

#include "llm_backend.hh"

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


class SessionStore {
public:
    SessionStore();

    // Set the directory for session files
    void SetDirectory(const std::string& dir);

    // Save session history to disk (Markdown format)
    bool SaveSession(
        const std::string& session_id,
        const std::vector<LlmMessage>& history);

    // Load session history from disk (Markdown or
    // legacy JSON with auto-migration)
    std::vector<LlmMessage> LoadSession(
        const std::string& session_id);

    // Delete a session file
    void DeleteSession(
        const std::string& session_id);

    // Skill execution logging
    void LogSkillExecution(
        const std::string& session_id,
        const std::string& skill_name,
        const nlohmann::json& args,
        const std::string& result,
        int duration_ms);

    // Token usage logging
    void LogTokenUsage(
        const std::string& session_id,
        const std::string& model_name,
        int prompt_tokens,
        int completion_tokens);

    // Load token usage for a session
    TokenUsageSummary LoadTokenUsage(
        const std::string& session_id);

private:
    // Find existing session file by scanning
    // dir for *-{session_id}.md pattern
    std::string FindSessionFile(
        const std::string& dir,
        const std::string& session_id) const;

    std::string GetSessionPath(
        const std::string& session_id) const;
    std::string GetLegacySessionPath(
        const std::string& session_id) const;
    std::string GetLogsDir() const;
    std::string GetUsageDir() const;

    // Get current date as YYYY-MM-DD string
    static std::string GetDatePrefix();

    // Markdown serialization
    std::string MessagesToMarkdown(
        const std::vector<LlmMessage>& history) const;
    std::vector<LlmMessage> MarkdownToMessages(
        const std::string& content) const;

    // Legacy JSON support
    static nlohmann::json MessageToJson(
        const LlmMessage& msg);
    static LlmMessage JsonToMessage(
        const nlohmann::json& j);

    // Get current timestamp as ISO string
    static std::string GetTimestamp();

    // Ensure a directory exists (mkdir -p)
    static void EnsureDir(const std::string& dir);

    // Atomic file write (write .tmp then rename)
    static bool AtomicWrite(
        const std::string& path,
        const std::string& content);

    std::string sessions_dir_;

    // Max file size in bytes (512KB)
    static constexpr size_t kMaxFileSize =
        512 * 1024;
};

} // namespace tizenclaw

#endif  // __SESSION_STORE_H__
