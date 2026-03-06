#ifndef __AGENT_CORE_H__
#define __AGENT_CORE_H__

#include <map>
#include <string>
#include <vector>
#include <memory>
#include <future>
#include <json.hpp>

#include "container_engine.hh"
#include "llm_backend.hh"
#include "session_store.hh"
#include <mutex>

namespace tizenclaw {


class AgentCore {
public:
    AgentCore();
    ~AgentCore();

    bool Initialize();
    void Shutdown();

    // Process a prompt, returns response text
    std::string ProcessPrompt(
        const std::string& session_id,
        const std::string& prompt,
        std::function<void(const std::string&)> on_chunk = nullptr);

    // Clear a session from memory and disk
    void ClearSession(
        const std::string& session_id);

    // Direct skill execution for MCP (bypasses LLM,
    // but still uses container isolation)
    std::string ExecuteSkillForMcp(
        const std::string& skill_name,
        const nlohmann::json& args);

private:
    // Execute a skill and return its JSON output
    std::string ExecuteSkill(
        const std::string& skill_name,
        const nlohmann::json& args);

    // Execute arbitrary Python code (built-in tool)
    std::string ExecuteCode(
        const std::string& code);

    // Execute file operations (built-in tool)
    std::string ExecuteFileOp(
        const std::string& operation,
        const std::string& path,
        const std::string& content);

    // Load skill manifests as tool declarations
    std::vector<LlmToolDecl>
    LoadSkillDeclarations();

    // Load system prompt from config or file
    std::string LoadSystemPrompt(
        const nlohmann::json& config);

    // Build final system prompt with dynamic skill list
    std::string BuildSystemPrompt(
        const std::vector<LlmToolDecl>& tools);

    // Trim session history to kMaxHistorySize
    void TrimHistory(
        const std::string& session_id);

    std::unique_ptr<ContainerEngine> m_container;
    std::unique_ptr<LlmBackend> m_backend;
    bool m_initialized;

    // System prompt loaded from external file
    std::string m_system_prompt;

    // Session-based conversation history
    std::map<std::string,
             std::vector<LlmMessage>> m_sessions;
    std::mutex session_mutex_; // Protects m_sessions

    static constexpr size_t kMaxHistorySize = 20;
    static constexpr int kMaxIterations = 5;

    SessionStore session_store_;
};

} // namespace tizenclaw

#endif // __AGENT_CORE_H__
