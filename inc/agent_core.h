#ifndef __AGENT_CORE_H__
#define __AGENT_CORE_H__

#include <string>
#include <vector>
#include <memory>
#include <json.hpp>

#include "container_engine.h"

class AgentCore {
public:
    AgentCore();
    ~AgentCore();

    // Initialize the core engine (e.g., load configs, prepare for MCP)
    bool Initialize();

    // Shutdown and cleanup
    void Shutdown();

    // Process an incoming prompt or intent via AppControl
    void ProcessPrompt(const std::string& prompt);

private:
    std::string QueryGemini(const std::string& prompt_text);
    bool ExecuteSkill(const std::string& skill_name, const nlohmann::json& args);

    std::unique_ptr<ContainerEngine> m_container;
    bool m_initialized;
    std::string m_gemini_api_key;
};

#endif // __AGENT_CORE_H__
