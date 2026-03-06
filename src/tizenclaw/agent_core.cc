#include <curl/curl.h>
#include <fstream>
#include <iostream>
#include <dirent.h>
#include <sys/stat.h>

#include "agent_core.hh"
#include "../common/logging.hh"

namespace tizenclaw {


AgentCore::AgentCore()
    : m_container(new ContainerEngine()),
      m_initialized(false) {
}

AgentCore::~AgentCore() {
  Shutdown();
}

bool AgentCore::Initialize() {
  if (m_initialized) return true;

  LOG(INFO) << "AgentCore Initializing...";

  if (!m_container->Initialize()) {
    LOG(ERROR) << "Failed to initialize ContainerEngine";
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
      LOG(ERROR) << "Failed to parse " << config_path << ": " << e.what();
    }
  }

  // Fallback: try legacy gemini_api_key.txt
  if (llm_config.empty()) {
    LOG(WARNING) << "llm_config.json not found, using legacy gemini key file";
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
    LOG(ERROR) << "Failed to create LLM backend: " << backend_name;
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
    LOG(ERROR) << "Failed to init backend: " << backend_name;
    m_backend.reset();
    return false;
  }

  curl_global_init(CURL_GLOBAL_DEFAULT);

  LOG(INFO) << "AgentCore initialized with backend: " << m_backend->GetName();
  m_initialized = true;
  return true;
}

void AgentCore::Shutdown() {
  if (!m_initialized) return;

  LOG(INFO) << "AgentCore Shutting down...";

  // Save all sessions before shutting down
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    for (auto& [sid, history] : m_sessions) {
      session_store_.SaveSession(sid, history);
    }
    m_sessions.clear();
  }
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
    const std::string& prompt,
    std::function<void(const std::string&)> on_chunk) {
  if (!m_initialized || !m_backend) {
    LOG(ERROR) << "AgentCore not initialized.";
    return "Error: AgentCore is not initialized.";
  }

  LOG(INFO) << "ProcessPrompt [" << session_id << "]: " << prompt;

  auto tools = LoadSkillDeclarations();

  std::vector<LlmMessage> local_history;
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    // Load from disk if not in memory
    if (m_sessions.find(session_id) == m_sessions.end()) {
      auto loaded = session_store_.LoadSession(session_id);
      if (!loaded.empty()) {
        m_sessions[session_id] = std::move(loaded);
      }
    }

    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = prompt;
    m_sessions[session_id].push_back(user_msg);
    TrimHistory(session_id);

    // Copy history to local variable to avoid holding lock during LLM API call
    local_history = m_sessions[session_id];
  }

  int iterations = 0;
  std::string last_text;

  while (iterations < kMaxIterations) {
    // Query LLM backend without holding lock
    LlmResponse resp = m_backend->Chat(local_history, tools, on_chunk);

    if (!resp.success) {
      LOG(ERROR) << "LLM error: "
                 << resp.error_message;
      // Rollback: remove the user message to
      // prevent corrupted history from poisoning
      // subsequent requests.
      {
        std::lock_guard<std::mutex> lock(
            session_mutex_);
        if (!m_sessions[session_id].empty()) {
          m_sessions[session_id].pop_back();
        }
      }
      return "Error: " + resp.error_message;
    }

    if (!resp.HasToolCalls()) {
      // No more tool calls — final text response
      LlmMessage model_msg;
      model_msg.role = "assistant";
      model_msg.text = resp.text;
      
      {
        std::lock_guard<std::mutex> lock(session_mutex_);
        m_sessions[session_id].push_back(model_msg);
        TrimHistory(session_id);
        
        session_store_.SaveSession(
            session_id, m_sessions[session_id]);
      }

      LOG(INFO) << "Final response: " << resp.text;

      return resp.text.empty() ? "No response text." : resp.text;
    }

    // Handle tool calls (function calling)
    last_text = resp.text;
    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = resp.text;
    assistant_msg.tool_calls = resp.tool_calls;
    
    // Add to local history and global session
    local_history.push_back(assistant_msg);
    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      m_sessions[session_id].push_back(assistant_msg);
    }

    LOG(INFO) << "Iteration " << (iterations + 1) << ": Executing " << resp.tool_calls.size() << " tools in parallel";

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
        if (tc.name == "execute_code") {
          std::string code =
              tc.args.value("code", "");
          r.output = ExecuteCode(code);
        } else {
          r.output = ExecuteSkill(
              tc.name, tc.args);
        }
        return r;
      }));
    }

    std::vector<LlmMessage> tool_msgs;
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
      
      tool_msgs.push_back(tool_msg);
      local_history.push_back(tool_msg);
    }

    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      m_sessions[session_id].insert(
          m_sessions[session_id].end(),
          tool_msgs.begin(),
          tool_msgs.end());
      TrimHistory(session_id);
    }
    iterations++;
  }

  LOG(WARNING) << "Reached max tool iterations (" << kMaxIterations << ")";

  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    session_store_.SaveSession(
        session_id, m_sessions[session_id]);
  }

  return last_text.empty() ? "Task partially completed (reached iteration limit)." : last_text;
}

std::string AgentCore::ExecuteSkill(
    const std::string& skill_name,
    const nlohmann::json& args) {
  LOG(INFO) << "Executing skill: " << skill_name;

  std::string arg_str = args.dump();
  std::string response =
      m_container->ExecuteSkill(skill_name,
                                arg_str);

  if (response.empty()) {
    LOG(ERROR) << "Skill execution failed";
    return "{\"error\": \"Skill failed\"}";
  }

  LOG(INFO) << "Skill output: " << response;
  return response;
}

std::string AgentCore::ExecuteCode(
    const std::string& code) {
  LOG(INFO) << "ExecuteCode: "
            << code.size() << " chars";

  std::string response =
      m_container->ExecuteCode(code);

  if (response.empty()) {
    LOG(ERROR) << "Code execution failed";
    return "{\"error\": \"Code execution failed\"}";
  }

  LOG(INFO) << "Code output: " << response;
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
      LOG(WARNING) << "Failed to parse manifest: " << manifest_path;
    }
  }
  closedir(dir);

  // Built-in tool: execute_code
  LlmToolDecl code_tool;
  code_tool.name = "execute_code";
  code_tool.description =
      "Execute arbitrary Python code on the Tizen "
      "device. Use this when no existing skill/tool "
      "can accomplish the task. The code MUST print "
      "a JSON result to stdout as the last line. "
      "Available: ctypes for Tizen C-API, os, "
      "subprocess, json, sys. "
      "Libraries at /tizen_libs or system path.";
  code_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"code", {
              {"type", "string"},
              {"description",
               "Python code to execute on the "
               "Tizen device"}
          }}
      }},
      {"required", nlohmann::json::array({"code"})}
  };
  tools.push_back(code_tool);

  return tools;
}

void AgentCore::TrimHistory(
    const std::string& session_id) {
  // MUST be called with session_mutex_ held
  auto& history = m_sessions[session_id];
  while (history.size() > kMaxHistorySize) {
    history.erase(history.begin());
  }
}

void AgentCore::ClearSession(
    const std::string& session_id) {
  std::lock_guard<std::mutex> lock(session_mutex_);
  m_sessions.erase(session_id);
  session_store_.DeleteSession(session_id);
}

} // namespace tizenclaw
