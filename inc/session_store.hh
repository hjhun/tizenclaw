#ifndef __SESSION_STORE_H__
#define __SESSION_STORE_H__

#include <string>
#include <vector>
#include <json.hpp>

#include "llm_backend.hh"

class SessionStore {
public:
    SessionStore();

    // Set the directory for session files
    void SetDirectory(const std::string& dir);

    // Save session history to disk
    bool SaveSession(
        const std::string& session_id,
        const std::vector<LlmMessage>& history);

    // Load session history from disk
    std::vector<LlmMessage> LoadSession(
        const std::string& session_id);

    // Delete a session file
    void DeleteSession(
        const std::string& session_id);

private:
    std::string GetSessionPath(
        const std::string& session_id) const;

    // Serialize/deserialize helpers
    static nlohmann::json MessageToJson(
        const LlmMessage& msg);
    static LlmMessage JsonToMessage(
        const nlohmann::json& j);

    std::string sessions_dir_;

    // Max file size in bytes (512KB)
    static constexpr size_t kMaxFileSize =
        512 * 1024;
};

#endif  // __SESSION_STORE_H__
