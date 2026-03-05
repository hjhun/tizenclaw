#include <dlog.h>
#include <curl/curl.h>
#include <fstream>
#include <iostream>
#include <dirent.h>
#include <sys/stat.h>

#include "agent_core.hh"

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_AgentCore"

AgentCore::AgentCore()
    : m_container(new ContainerEngine()),
      m_initialized(false) {
}

AgentCore::~AgentCore() {
  Shutdown();
}

bool AgentCore::Initialize() {
  if (m_initialized) return true;

  dlog_print(DLOG_INFO, LOG_TAG,
             "AgentCore Initializing...");

  if (!m_container->Initialize()) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Failed to initialize ContainerEngine");
    return false;
  }

  // Load LLM config
  const char* env_path = std::getenv("TIZENCLAW_CONFIG_PATH");
  std::string config_path = env_path ? env_path : "/opt/usr/share/tizenclaw/llm_config.json";
  nlohmann::json llm_config;

  std::ifstream cf(config_path);
  if (cf.is_open()) {
    try {
      cf >> llm_config;
      cf.close();
    } catch (const std::exception& e) {
      dlog_print(DLOG_ERROR, LOG_TAG,
                 "Failed to parse %s: %s",
                 config_path.c_str(), e.what());
    }
  }

  // Fallback: try legacy gemini_api_key.txt
  if (llm_config.empty()) {
    dlog_print(DLOG_WARN, LOG_TAG,
               "llm_config.json not found, "
               "using legacy gemini key file");
    std::string api_key;
    std::ifstream kf(
        "/opt/usr/share/tizenclaw/"
        "gemini_api_key.txt");
    if (kf.is_open()) {
      std::getline(kf, api_key);
      kf.close();
    }
    llm_config = {
        {"active_backend", "gemini"},
        {"backends", {
            {"gemini", {
                {"api_key", api_key},
                {"model", "gemini-2.5-flash"}
            }}
        }}
    };
  }

  // Create backend from config
  std::string backend_name =
      llm_config.value("active_backend", "gemini");

  m_backend =
      LlmBackendFactory::Create(backend_name);
  if (!m_backend) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Failed to create LLM backend: %s",
               backend_name.c_str());
    return false;
  }

  // Get provider-specific config
  nlohmann::json backend_config;
  if (llm_config.contains("backends") &&
      llm_config["backends"]
          .contains(backend_name)) {
    backend_config =
        llm_config["backends"][backend_name];
  }

  // For xAI, inject provider_name so OpenAiBackend
  // knows its identity
  if (backend_name == "xai" ||
      backend_name == "grok") {
    backend_config["provider_name"] = "xai";
    if (!backend_config.contains("endpoint")) {
      backend_config["endpoint"] =
          "https://api.x.ai/v1";
    }
  }

  if (!m_backend->Initialize(backend_config)) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Failed to init backend: %s",
               backend_name.c_str());
    m_backend.reset();
    return false;
  }

  curl_global_init(CURL_GLOBAL_DEFAULT);

  dlog_print(DLOG_INFO, LOG_TAG,
             "AgentCore initialized with "
             "backend: %s",
             m_backend->GetName().c_str());
  m_initialized = true;
  return true;
}

void AgentCore::Shutdown() {
  if (!m_initialized) return;

  dlog_print(DLOG_INFO, LOG_TAG,
             "AgentCore Shutting down...");

  // Save all sessions before shutting down
  for (auto& [sid, history] : m_sessions) {
    session_store_.SaveSession(sid, history);
  }
  m_sessions.clear();
  if (m_backend) {
    m_backend->Shutdown();
    m_backend.reset();
  }
  m_container.reset();
  curl_global_cleanup();

  m_initialized = false;
}

std::string AgentCore::ProcessPrompt(
    const std::string& session_id,
    const std::string& prompt) {
  if (!m_initialized || !m_backend) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "AgentCore not initialized.");
    return "Error: AgentCore is not initialized.";
  }

  dlog_print(DLOG_INFO, LOG_TAG,
             "ProcessPrompt [%s]: %s",
             session_id.c_str(), prompt.c_str());

  auto tools = LoadSkillDeclarations();

  // Add user message to session history
  // Load from disk if not in memory
  if (m_sessions.find(session_id) ==
      m_sessions.end()) {
    auto loaded =
        session_store_.LoadSession(session_id);
    if (!loaded.empty()) {
      m_sessions[session_id] = std::move(loaded);
    }
  }

  LlmMessage user_msg;
  user_msg.role = "user";
  user_msg.text = prompt;
  m_sessions[session_id].push_back(user_msg);
  TrimHistory(session_id);

  int iterations = 0;
  std::string last_text;

  while (iterations < kMaxIterations) {
    auto& history = m_sessions[session_id];

    // Query LLM backend
    LlmResponse resp = m_backend->Chat(history, tools);

    if (!resp.success) {
      dlog_print(DLOG_ERROR, LOG_TAG, "LLM error: %s", resp.error_message.c_str());
      return "Error: " + resp.error_message;
    }

    if (!resp.HasToolCalls()) {
      // No more tool calls — final text response
      LlmMessage model_msg;
      model_msg.role = "assistant";
      model_msg.text = resp.text;
      history.push_back(model_msg);
      TrimHistory(session_id);

      dlog_print(DLOG_INFO, LOG_TAG, "Final response: %s", resp.text.c_str());

      // Save session to disk
      session_store_.SaveSession(
          session_id, history);

      return resp.text.empty() ? "No response text." : resp.text;
    }

    // Handle tool calls (function calling)
    last_text = resp.text;
    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = resp.text;
    assistant_msg.tool_calls = resp.tool_calls;
    history.push_back(assistant_msg);

    dlog_print(DLOG_INFO, LOG_TAG, "Iteration %d: Executing %zu tools in parallel",
               iterations + 1, resp.tool_calls.size());

    // Execute multiple tools in parallel using std::async
    struct ToolExecResult {
      std::string id;
      std::string name;
      std::string output;
    };
    std::vector<std::future<ToolExecResult>> futures;
    for (auto& tc : resp.tool_calls) {
      futures.push_back(std::async(std::launch::async,
          [this, tc]() {
        ToolExecResult r;
        r.id = tc.id;
        r.name = tc.name;
        r.output = ExecuteSkill(tc.name, tc.args);
        return r;
      }));
    }

    // Collect results with accurate tool_call_id
    for (auto& f : futures) {
      auto result = f.get();
      LlmMessage tool_msg;
      tool_msg.role = "tool";
      tool_msg.tool_name = result.name;
      tool_msg.tool_call_id = result.id;
      try {
        tool_msg.tool_result =
            nlohmann::json::parse(result.output);
      } catch (...) {
        tool_msg.tool_result =
            {{"output", result.output}};
      }
      history.push_back(tool_msg);
    }

    TrimHistory(session_id);
    iterations++;
  }

  dlog_print(DLOG_WARN, LOG_TAG, "Reached max tool iterations (%d)", kMaxIterations);

  // Save session even on iteration limit
  session_store_.SaveSession(
      session_id, m_sessions[session_id]);

  return last_text.empty() ? "Task partially completed (reached iteration limit)." : last_text;
}

std::string AgentCore::ExecuteSkill(
    const std::string& skill_name,
    const nlohmann::json& args) {
  dlog_print(DLOG_INFO, LOG_TAG,
             "Executing skill: %s",
             skill_name.c_str());

  std::string arg_str = args.dump();
  std::string response =
      m_container->ExecuteSkill(skill_name,
                                arg_str);

  if (response.empty()) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Skill execution failed");
    return "{\"error\": \"Skill failed\"}";
  }

  dlog_print(DLOG_INFO, LOG_TAG,
             "Skill output: %s",
             response.c_str());
  return response;
}

std::vector<LlmToolDecl>
AgentCore::LoadSkillDeclarations() {
  std::vector<LlmToolDecl> tools;
  const std::string skills_dir =
      "/opt/usr/share/tizenclaw/skills";

  DIR* dir = opendir(skills_dir.c_str());
  if (!dir) return tools;

  struct dirent* ent;
  while ((ent = readdir(dir)) != nullptr) {
    if (ent->d_name[0] == '.') continue;
    std::string manifest_path =
        skills_dir + "/" + ent->d_name +
        "/manifest.json";
    std::ifstream mf(manifest_path);
    if (!mf.is_open()) continue;

    try {
      nlohmann::json j;
      mf >> j;
      if (j.contains("parameters")) {
        LlmToolDecl t;
        t.name =
            j.value("name", ent->d_name);
        t.description =
            j.value("description", "");
        t.parameters = j["parameters"];
        tools.push_back(t);
      }
    } catch (...) {
      dlog_print(DLOG_WARN, LOG_TAG,
                 "Failed to parse manifest: %s",
                 manifest_path.c_str());
    }
  }
  closedir(dir);
  return tools;
}

void AgentCore::TrimHistory(
    const std::string& session_id) {
  auto& history = m_sessions[session_id];
  while (history.size() > kMaxHistorySize) {
    history.erase(history.begin());
  }
}
