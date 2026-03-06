#ifndef __AGENT_CORE_H__
#define __AGENT_CORE_H__

#include <atomic>
#include <map>
#include <string>
#include <vector>
#include <memory>
#include <future>
#include <json.hpp>

#include "container_engine.hh"
#include "llm_backend.hh"
#include "session_store.hh"
#include "tool_policy.hh"
#include "task_scheduler.hh"
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

    // Set TaskScheduler reference (called by daemon)
    void SetScheduler(TaskScheduler* scheduler) {
      scheduler_ = scheduler;
    }

    // Access session store (for IPC usage queries)
    SessionStore& GetSessionStore() {
      return session_store_;
    }

    // Reload skill declarations (thread-safe)
    // Called by SkillWatcher on manifest changes
    void ReloadSkills();

    // Execute session management operations
    // (create_session, list_sessions,
    //  send_to_session)
    std::string ExecuteSessionOp(
        const std::string& operation,
        const nlohmann::json& args,
        const std::string& caller_session);

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

    // Execute task scheduler operations
    std::string ExecuteTaskOp(
        const std::string& operation,
        const nlohmann::json& args);

    // Load skill manifests as tool declarations
    std::vector<LlmToolDecl>
    LoadSkillDeclarations();

    // Load system prompt from config or file
    std::string LoadSystemPrompt(
        const nlohmann::json& config);

    // Build final system prompt with dynamic
    // skill list
    std::string BuildSystemPrompt(
        const std::vector<LlmToolDecl>& tools);

    // Try fallback backends on primary failure
    LlmResponse TryFallbackBackends(
        const std::vector<LlmMessage>& history,
        const std::vector<LlmToolDecl>& tools,
        std::function<void(
            const std::string&)> on_chunk,
        const std::string& system_prompt);

    // Compact history via LLM summarization
    // MUST be called with session_mutex_ held
    void CompactHistory(
        const std::string& session_id);

    // Trim session history (compaction + FIFO)
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

    // Per-session system prompt overrides
    // session_id → custom system_prompt
    std::map<std::string, std::string>
        session_prompts_;

    static constexpr size_t kMaxHistorySize = 30;
    static constexpr size_t kCompactionThreshold
        = 15;
    static constexpr size_t kCompactionCount = 10;

    SessionStore session_store_;
    ToolPolicy tool_policy_;

    // Cached skill declarations
    std::vector<LlmToolDecl> cached_tools_;
    std::atomic<bool> cached_tools_loaded_{false};
    std::mutex tools_mutex_;  // Protects cached_tools_

    // Model fallback configuration
    std::vector<std::string> fallback_names_;
    nlohmann::json llm_config_;

    // Task scheduler (owned by daemon)
    TaskScheduler* scheduler_ = nullptr;

    // Get session-specific system prompt
    // (falls back to global m_system_prompt)
    std::string GetSessionPrompt(
        const std::string& session_id,
        const std::vector<LlmToolDecl>& tools);
};

} // namespace tizenclaw

#endif // __AGENT_CORE_H__
