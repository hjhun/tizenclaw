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

class AgentCore {
public:
    AgentCore();
    ~AgentCore();

    bool Initialize();
    void Shutdown();

    // Process a prompt, returns response text
    std::string ProcessPrompt(
        const std::string& session_id,
        const std::string& prompt);

private:
    // Execute a skill and return its JSON output
    std::string ExecuteSkill(
        const std::string& skill_name,
        const nlohmann::json& args);

    // Load skill manifests as tool declarations
    std::vector<LlmToolDecl>
    LoadSkillDeclarations();

    // Trim session history to kMaxHistorySize
    void TrimHistory(
        const std::string& session_id);

    std::unique_ptr<ContainerEngine> m_container;
    std::unique_ptr<LlmBackend> m_backend;
    bool m_initialized;

    // Session-based conversation history
    std::map<std::string,
             std::vector<LlmMessage>> m_sessions;
    static constexpr size_t kMaxHistorySize = 20;
    static constexpr int kMaxIterations = 5;

    SessionStore session_store_;
};

#endif // __AGENT_CORE_H__
