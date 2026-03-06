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
  std::string config_path = env_path ? env_path : "/opt/usr/share/tizenclaw/config/llm_config.json";
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
        "/opt/usr/share/tizenclaw/config/"
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

  // Load system prompt
  m_system_prompt = LoadSystemPrompt(llm_config);
  if (!m_system_prompt.empty()) {
    LOG(INFO) << "System prompt loaded (" << m_system_prompt.size() << " chars)";
  } else {
    LOG(WARNING) << "No system prompt configured";
  }

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
    // Build system prompt with dynamic skill list
    std::string full_prompt = BuildSystemPrompt(tools);

    // Query LLM backend without holding lock
    LlmResponse resp = m_backend->Chat(
        local_history, tools, on_chunk, full_prompt);

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
        } else if (tc.name == "file_manager") {
          std::string op =
              tc.args.value("operation", "");
          std::string path =
              tc.args.value("path", "");
          std::string content =
              tc.args.value("content", "");
          r.output = ExecuteFileOp(
              op, path, content);
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

std::string AgentCore::ExecuteFileOp(
    const std::string& operation,
    const std::string& path,
    const std::string& content) {
  LOG(INFO) << "ExecuteFileOp: op=" << operation
            << " path=" << path;

  std::string response =
      m_container->ExecuteFileOp(
          operation, path, content);

  if (response.empty()) {
    LOG(ERROR) << "File operation failed";
    return "{\"error\": \"File operation failed\"}";
  }

  LOG(INFO) << "FileOp output: " << response;
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

  // Built-in tool: file_manager
  LlmToolDecl file_tool;
  file_tool.name = "file_manager";
  file_tool.description =
      "Manage files on the Tizen device. "
      "Create, read, delete files or list "
      "directory contents. Paths MUST start "
      "with /skills/ or /data/ — other paths "
      "are rejected. Use /skills/ to save new "
      "skill scripts, /data/ for persistent data.";
  file_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"operation", {
              {"type", "string"},
              {"enum", nlohmann::json::array(
                  {"write_file", "read_file",
                   "delete_file", "list_dir"})},
              {"description",
               "The file operation to perform"}
          }},
          {"path", {
              {"type", "string"},
              {"description",
               "File or directory path. Must start "
               "with /skills/ or /data/"}
          }},
          {"content", {
              {"type", "string"},
              {"description",
               "File content (for write_file only)"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"operation", "path"})}
  };
  tools.push_back(file_tool);

  return tools;
}

std::string AgentCore::LoadSystemPrompt(
    const nlohmann::json& config) {
  // Priority 1: Inline system_prompt in config
  if (config.contains("system_prompt") &&
      config["system_prompt"].is_string()) {
    std::string prompt =
        config["system_prompt"].get<std::string>();
    if (!prompt.empty()) {
      LOG(INFO) << "System prompt loaded from config (inline)";
      return prompt;
    }
  }

  // Priority 2: system_prompt_file path in config
  if (config.contains("system_prompt_file") &&
      config["system_prompt_file"].is_string()) {
    std::string file_path =
        config["system_prompt_file"]
            .get<std::string>();
    std::ifstream pf(file_path);
    if (pf.is_open()) {
      std::string content(
          (std::istreambuf_iterator<char>(pf)),
          std::istreambuf_iterator<char>());
      pf.close();
      if (!content.empty()) {
        LOG(INFO) << "System prompt loaded from: "
                  << file_path;
        return content;
      }
    }
  }

  // Priority 3: Default file path
  const std::string default_path =
      "/opt/usr/share/tizenclaw/config/"
      "system_prompt.txt";
  std::ifstream df(default_path);
  if (df.is_open()) {
    std::string content(
        (std::istreambuf_iterator<char>(df)),
        std::istreambuf_iterator<char>());
    df.close();
    if (!content.empty()) {
      LOG(INFO) << "System prompt loaded from default: "
                << default_path;
      return content;
    }
  }

  // Priority 4: Hardcoded fallback
  LOG(INFO) << "Using hardcoded default system prompt";
  return
      "You are TizenClaw, an AI assistant running "
      "on a Tizen device. You can control the device "
      "using the available tools. Always respond in "
      "the same language as the user's message. "
      "Be concise and helpful.";
}

std::string AgentCore::BuildSystemPrompt(
    const std::vector<LlmToolDecl>& tools) {
  std::string prompt = m_system_prompt;

  // Build tool list string
  std::string tool_list;
  for (const auto& t : tools) {
    tool_list += "- " + t.name + ": "
        + t.description + "\n";
  }

  // Replace {{AVAILABLE_TOOLS}} placeholder
  const std::string placeholder =
      "{{AVAILABLE_TOOLS}}";
  size_t pos = prompt.find(placeholder);
  if (pos != std::string::npos) {
    prompt.replace(pos, placeholder.size(),
                   tool_list);
  } else if (!tool_list.empty()) {
    // If no placeholder, append tool list
    prompt += "\n\nAvailable tools:\n" + tool_list;
  }

  return prompt;
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

std::string AgentCore::ExecuteSkillForMcp(
    const std::string& skill_name,
    const nlohmann::json& args) {
  return ExecuteSkill(skill_name, args);
}

} // namespace tizenclaw
