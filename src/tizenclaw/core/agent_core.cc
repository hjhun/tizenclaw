#include <curl/curl.h>
#include <fstream>
#include <iostream>
#include <chrono>
#include <filesystem>
#include <sstream>
#include <sys/stat.h>
#include <thread>

#include "agent_core.hh"
#include "../infra/http_client.hh"
#include "../storage/audit_logger.hh"
#include "../infra/key_store.hh"
#include "../../common/logging.hh"

namespace tizenclaw {


AgentCore::AgentCore()
    : container_(
          std::make_unique<ContainerEngine>()),
      initialized_(false) {
}

AgentCore::~AgentCore() {
  Shutdown();
}

bool AgentCore::Initialize() {
  if (initialized_) return true;

  LOG(INFO) << "AgentCore Initializing...";

  if (!container_->Initialize()) {
    LOG(ERROR) << "Failed to initialize ContainerEngine";
    return false;
  }

  // Load LLM config
  const char* env_path = std::getenv("TIZENCLAW_CONFIG_PATH");
  std::string config_path = env_path
      ? env_path
      : "/opt/usr/share/tizenclaw/"
        "config/llm_config.json";
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

  backend_ =
      LlmBackendFactory::Create(backend_name);
  if (!backend_) {
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

  // Decrypt API key if encrypted
  if (backend_config.contains("api_key")) {
    std::string api_key =
        backend_config["api_key"]
            .get<std::string>();
    if (KeyStore::IsEncrypted(api_key)) {
      std::string decrypted =
          KeyStore::Decrypt(api_key);
      if (!decrypted.empty()) {
        backend_config["api_key"] = decrypted;
        LOG(INFO) << "API key decrypted for: "
                  << backend_name;
      } else {
        LOG(ERROR)
            << "Failed to decrypt API key";
      }
    }
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

  if (!backend_->Initialize(backend_config)) {
    LOG(ERROR) << "Failed to init backend: " << backend_name;
    backend_.reset();
    return false;
  }

  curl_global_init(CURL_GLOBAL_DEFAULT);

  // Load system prompt
  system_prompt_ = LoadSystemPrompt(llm_config);
  if (!system_prompt_.empty()) {
    LOG(INFO) << "System prompt loaded ("
              << system_prompt_.size()
              << " chars)";
  } else {
    LOG(WARNING) << "No system prompt configured";
  }

  // Load tool execution policy
  std::string policy_path =
      "/opt/usr/share/tizenclaw/config/"
      "tool_policy.json";
  tool_policy_.LoadConfig(policy_path);

  // Parse fallback backends from config
  if (llm_config.contains(
          "fallback_backends")) {
    for (auto& name :
         llm_config["fallback_backends"]) {
      std::string fb =
          name.get<std::string>();
      if (fb != backend_name) {
        fallback_names_.push_back(fb);
      }
    }
    if (!fallback_names_.empty()) {
      LOG(INFO)
          << "Fallback backends: "
          << fallback_names_.size()
          << " configured";
    }
  }
  llm_config_ = llm_config;

  // Audit: config loaded
  AuditLogger::Instance().Log(
      AuditLogger::MakeEvent(
          AuditEventType::kConfigChange,
          "",
          {{"backend",
            backend_->GetName()}}));

  LOG(INFO) << "AgentCore initialized with "
            << "backend: "
            << backend_->GetName();
  initialized_ = true;

  // Initialize embedding store for RAG
  std::string rag_db =
      std::string(APP_DATA_DIR) +
      "/rag/embeddings.db";
  // Ensure rag directory exists
  std::string rag_dir =
      std::string(APP_DATA_DIR) + "/rag";
  mkdir(rag_dir.c_str(), 0755);
  if (embedding_store_.Initialize(rag_db)) {
    LOG(INFO) << "RAG embedding store ready";
  } else {
    LOG(WARNING) << "RAG embedding store "
                 << "init failed (non-fatal)";
  }

  // Initialize supervisor engine
  supervisor_ =
      std::make_unique<SupervisorEngine>(this);
  std::string roles_path =
      std::string(APP_DATA_DIR) +
      "/config/agent_roles.json";
  if (supervisor_->LoadRoles(roles_path)) {
    LOG(INFO)
        << "Supervisor engine ready with "
        << supervisor_->GetRoleNames().size()
        << " roles";
  } else {
    LOG(WARNING)
        << "Supervisor engine: no roles "
        << "configured (non-fatal)";
  }

  // Initialize pipeline executor
  pipeline_executor_ =
      std::make_unique<PipelineExecutor>(this);
  pipeline_executor_->LoadPipelines();
  LOG(INFO) << "Pipeline executor ready";

#ifdef TIZEN_ACTION_ENABLED
  // Initialize Tizen Action Framework bridge
  action_bridge_ =
      std::make_unique<ActionBridge>();
  if (action_bridge_->Start()) {
    // Sync action schemas to MD files
    action_bridge_->SyncActionSchemas();
    // React to action install/uninstall/update
    action_bridge_->SetChangeCallback([this]() {
      LOG(INFO)
          << "Action schemas changed, "
          << "reloading tools";
      cached_tools_loaded_.store(false);
    });
    LOG(INFO)
        << "Tizen Action bridge ready";
  } else {
    LOG(WARNING)
        << "Tizen Action bridge init failed "
        << "(non-fatal)";
    action_bridge_.reset();
  }
#endif

  return true;
}

void AgentCore::Shutdown() {
  if (!initialized_) return;

  LOG(INFO) << "AgentCore Shutting down...";

  // Save all sessions before shutting down
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    for (auto& [sid, history] : sessions_) {
      session_store_.SaveSession(sid, history);
    }
    sessions_.clear();
  }
  if (backend_) {
    backend_->Shutdown();
    backend_.reset();
  }
#ifdef TIZEN_ACTION_ENABLED
  if (action_bridge_) {
    action_bridge_->Stop();
    action_bridge_.reset();
  }
#endif
  embedding_store_.Close();
  container_.reset();
  curl_global_cleanup();

  initialized_ = false;
}

std::string AgentCore::ProcessPrompt(
    const std::string& session_id,
    const std::string& prompt,
    std::function<void(const std::string&)> on_chunk) {
  if (!initialized_ || !backend_) {
    LOG(ERROR) << "AgentCore not initialized.";
    return "Error: AgentCore is not initialized.";
  }

  LOG(INFO) << "ProcessPrompt [" << session_id << "]: " << prompt;

  auto tools = LoadSkillDeclarations();

  std::vector<LlmMessage> local_history;
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    // Load from disk if not in memory
    if (!sessions_.contains(session_id)) {
      auto loaded = session_store_.LoadSession(session_id);
      if (!loaded.empty()) {
        sessions_[session_id] = std::move(loaded);
      }
    }

    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = prompt;
    sessions_[session_id].push_back(user_msg);
    TrimHistory(session_id);

    // Copy history to local variable to avoid holding lock during LLM API call
    local_history = sessions_[session_id];
  }

  // Sanitize: strip orphaned tool messages
  // before sending to LLM (prevents HTTP 400)
  SessionStore::SanitizeHistory(local_history);

  int iterations = 0;
  std::string last_text;
  int max_iter =
      tool_policy_.GetMaxIterations();

  // Reset idle tracking for this prompt
  tool_policy_.ResetIdleTracking(session_id);

  while (iterations < max_iter) {
    // Build session-specific system prompt
    std::string full_prompt =
        GetSessionPrompt(session_id, tools);

    // Query LLM backend without holding lock
    LlmResponse resp = backend_->Chat(
        local_history, tools, on_chunk,
        full_prompt);

    // Track LLM call in health metrics
    if (health_monitor_)
      health_monitor_->IncrementLlmCallCount();

    if (!resp.success) {
      LOG(ERROR) << "LLM error: "
                 << resp.error_message;

      // Try fallback backends
      resp = TryFallbackBackends(
          local_history, tools, on_chunk,
          full_prompt);

      if (!resp.success) {
        // All backends failed — track error
        if (health_monitor_)
          health_monitor_->IncrementErrorCount();
        // Rollback: remove the user message
        {
          std::lock_guard<std::mutex> lock(
              session_mutex_);
          if (!sessions_[session_id]
                   .empty()) {
            sessions_[session_id]
                .pop_back();
          }
        }
        return "Error: "
               + resp.error_message;
      }
    }

    // Log token usage
    if (resp.total_tokens > 0) {
      session_store_.LogTokenUsage(
          session_id,
          backend_->GetName(),
          resp.prompt_tokens,
          resp.completion_tokens);
      LOG(INFO) << "Tokens: prompt="
                << resp.prompt_tokens
                << " completion="
                << resp.completion_tokens
                << " total="
                << resp.total_tokens;
    }

    if (!resp.HasToolCalls()) {
      // No more tool calls — final text response
      LlmMessage model_msg;
      model_msg.role = "assistant";
      model_msg.text = resp.text;
      
      {
        std::lock_guard<std::mutex> lock(session_mutex_);
        sessions_[session_id].push_back(model_msg);
        TrimHistory(session_id);
        
        session_store_.SaveSession(
            session_id, sessions_[session_id]);
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
      sessions_[session_id].push_back(assistant_msg);
    }

    LOG(INFO)
        << "Iteration "
        << (iterations + 1)
        << ": Executing "
        << resp.tool_calls.size()
        << " tools in parallel";

    // Execute multiple tools in parallel using std::async
    struct ToolExecResult {
      std::string id;
      std::string name;
      std::string output;
    };
    std::vector<std::future<ToolExecResult>> futures;
    for (auto& tc : resp.tool_calls) {
      futures.push_back(std::async(
          std::launch::async,
          [this, tc, &session_id]() {
        ToolExecResult r;
        r.id = tc.id;
        r.name = tc.name;

        // Check tool execution policy
        std::string violation =
            tool_policy_.CheckPolicy(
                session_id, tc.name, tc.args);
        if (!violation.empty()) {
          LOG(WARNING)
              << "Tool blocked by policy: "
              << tc.name << " - "
              << violation;
          r.output = "{\"error\": \""
              + violation + "\"}";
          AuditLogger::Instance().Log(
              AuditLogger::MakeEvent(
                  AuditEventType::kToolBlocked,
                  session_id,
                  {{"skill", tc.name},
                   {"reason", violation}}));
          return r;
        }

        auto start =
            std::chrono::steady_clock::now();
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
        } else if (tc.name == "create_task" ||
                   tc.name == "list_tasks" ||
                   tc.name == "cancel_task") {
          r.output = ExecuteTaskOp(
              tc.name, tc.args);
        } else if (
            tc.name == "create_session" ||
            tc.name == "list_sessions" ||
            tc.name == "send_to_session") {
          r.output = ExecuteSessionOp(
              tc.name, tc.args,
              session_id);
        } else if (
            tc.name == "ingest_document" ||
            tc.name == "search_knowledge") {
          r.output = ExecuteRagOp(
              tc.name, tc.args);
        } else if (
            tc.name == "run_supervisor" ||
            tc.name == "list_agent_roles") {
          r.output = ExecuteSupervisorOp(
              tc.name, tc.args,
              session_id);
        } else if (
            tc.name == "create_pipeline" ||
            tc.name == "list_pipelines" ||
            tc.name == "run_pipeline" ||
            tc.name == "delete_pipeline") {
          r.output = ExecutePipelineOp(
              tc.name, tc.args,
              session_id);
        } else if (
            tc.name == "execute_action" ||
            tc.name.starts_with("action_")) {
          r.output = ExecuteActionOp(
              tc.name, tc.args);
        } else {
          r.output = ExecuteSkill(
              tc.name, tc.args);
        }
        auto elapsed =
            std::chrono::duration_cast<
                std::chrono::milliseconds>(
                std::chrono::steady_clock::now()
                - start).count();
        // Log skill execution
        session_store_.LogSkillExecution(
            session_id, tc.name, tc.args,
            r.output.substr(
                0, std::min((size_t)200,
                            r.output.size())),
            static_cast<int>(elapsed));
        AuditLogger::Instance().Log(
            AuditLogger::MakeEvent(
                AuditEventType::kToolExecution,
                session_id,
                {{"skill", tc.name},
                 {"duration",
                  std::to_string(elapsed)
                      + "ms"}}));
        // Track tool call in health metrics
        if (health_monitor_)
          health_monitor_->IncrementToolCallCount();
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
      sessions_[session_id].insert(
          sessions_[session_id].end(),
          tool_msgs.begin(),
          tool_msgs.end());
      TrimHistory(session_id);
    }

    // Check idle progress (no new results)
    std::ostringstream iter_sig;
    for (auto& tm : tool_msgs) {
      iter_sig << tm.tool_name << ":";
      if (!tm.tool_result.is_null()) {
        iter_sig << tm.tool_result.dump();
      }
      iter_sig << ";";
    }
    if (tool_policy_.CheckIdleProgress(
            session_id, iter_sig.str())) {
      LOG(WARNING)
          << "Idle loop detected in session: "
          << session_id;
      std::string idle_msg =
          "I've been running the same tools "
          "without making progress. "
          "Stopping to prevent an infinite "
          "loop. Please try a different "
          "approach.";

      LlmMessage stop_msg;
      stop_msg.role = "assistant";
      stop_msg.text = idle_msg;
      {
        std::lock_guard<std::mutex> lock(
            session_mutex_);
        sessions_[session_id].push_back(
            stop_msg);
        session_store_.SaveSession(
            session_id,
            sessions_[session_id]);
      }
      return idle_msg;
    }

    iterations++;
  }

  LOG(WARNING)
      << "Reached max tool iterations ("
      << max_iter << ")";

  {
    std::lock_guard<std::mutex> lock(
        session_mutex_);
    session_store_.SaveSession(
        session_id, sessions_[session_id]);
  }

  return last_text.empty()
      ? "Task partially completed "
        "(reached iteration limit)."
      : last_text;
}

std::string AgentCore::ExecuteSkill(
    const std::string& skill_name,
    const nlohmann::json& args) {
  LOG(INFO) << "Executing skill: " << skill_name;

  std::string arg_str = args.dump();
  std::string response =
      container_->ExecuteSkill(skill_name,
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
      container_->ExecuteCode(code);

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
      container_->ExecuteFileOp(
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
  // Return cached declarations after first load
  if (cached_tools_loaded_.load()) {
    std::lock_guard<std::mutex> lock(
        tools_mutex_);
    return cached_tools_;
  }

  std::lock_guard<std::mutex> lock(
      tools_mutex_);

  namespace fs = std::filesystem;

  std::vector<LlmToolDecl> tools;
  const std::string skills_dir =
      "/opt/usr/share/tizenclaw/skills";

  std::error_code ec;
  if (!fs::is_directory(skills_dir, ec))
    return tools;

  for (const auto& entry :
       fs::directory_iterator(skills_dir, ec)) {
    if (!entry.is_directory()) continue;
    auto dirname =
        entry.path().filename().string();
    if (dirname[0] == '.') continue;
    std::string manifest_path =
        entry.path() / "manifest.json";
    std::ifstream mf(manifest_path);
    if (!mf.is_open()) continue;

    try {
      nlohmann::json j;
      mf >> j;
      if (j.contains("parameters")) {
        LlmToolDecl t;
        t.name =
            j.value("name", dirname);
        t.description =
            j.value("description", "");
        t.parameters = j["parameters"];
        tools.push_back(t);
        // Load risk_level from manifest
        tool_policy_.LoadManifestRiskLevel(
            t.name, j);
      }
    } catch (...) {
      LOG(WARNING) << "Failed to parse "
                   << "manifest: "
                   << manifest_path;
    }
  }

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

  // Built-in tool: create_task
  LlmToolDecl create_task_tool;
  create_task_tool.name = "create_task";
  create_task_tool.description =
      "Create a scheduled task that runs "
      "automatically. Supports: "
      "'daily HH:MM' (every day), "
      "'interval Ns/Nm/Nh' (repeating), "
      "'once YYYY-MM-DD HH:MM' (one-shot), "
      "'weekly DAY HH:MM' (every week). "
      "The prompt will be sent to the LLM "
      "at the scheduled time.";
  create_task_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"schedule", {
              {"type", "string"},
              {"description",
               "Schedule expression, e.g. "
               "'daily 09:00', "
               "'interval 30m', "
               "'once 2026-03-10 14:00', "
               "'weekly mon 09:00'"}
          }},
          {"prompt", {
              {"type", "string"},
              {"description",
               "The prompt to execute at "
               "the scheduled time"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"schedule", "prompt"})}
  };
  tools.push_back(create_task_tool);

  // Built-in tool: list_tasks
  LlmToolDecl list_tasks_tool;
  list_tasks_tool.name = "list_tasks";
  list_tasks_tool.description =
      "List all scheduled tasks. Optionally "
      "filter by session_id.";
  list_tasks_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"session_id", {
              {"type", "string"},
              {"description",
               "Optional session ID to filter"}
          }}
      }},
      {"required", nlohmann::json::array()}
  };
  tools.push_back(list_tasks_tool);

  // Built-in tool: cancel_task
  LlmToolDecl cancel_task_tool;
  cancel_task_tool.name = "cancel_task";
  cancel_task_tool.description =
      "Cancel a scheduled task by its ID.";
  cancel_task_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"task_id", {
              {"type", "string"},
              {"description",
               "The task ID to cancel"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"task_id"})}
  };
  tools.push_back(cancel_task_tool);

  // Built-in tool: create_session
  LlmToolDecl create_session_tool;
  create_session_tool.name = "create_session";
  create_session_tool.description =
      "Create a new agent session with a custom "
      "system prompt. The new session operates "
      "independently with its own conversation "
      "history. Use this to delegate specialized "
      "tasks to a purpose-built agent.";
  create_session_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"name", {
              {"type", "string"},
              {"description",
               "Short name for the session "
               "(used as session_id prefix)"}
          }},
          {"system_prompt", {
              {"type", "string"},
              {"description",
               "Custom system prompt that "
               "defines the agent's role and "
               "behavior"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"name", "system_prompt"})}
  };
  tools.push_back(create_session_tool);

  // Built-in tool: list_sessions
  LlmToolDecl list_sessions_tool;
  list_sessions_tool.name = "list_sessions";
  list_sessions_tool.description =
      "List all active agent sessions with "
      "their names and system prompts.";
  list_sessions_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}
  };
  tools.push_back(list_sessions_tool);

  // Built-in tool: send_to_session
  LlmToolDecl send_to_session_tool;
  send_to_session_tool.name = "send_to_session";
  send_to_session_tool.description =
      "Send a message to another agent session "
      "and receive its response. The target "
      "session processes the message using its "
      "own system prompt and conversation "
      "history. Use this for inter-agent "
      "communication and task delegation.";
  send_to_session_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"target_session", {
              {"type", "string"},
              {"description",
               "The session_id of the target "
               "agent to send the message to"}
          }},
          {"message", {
              {"type", "string"},
              {"description",
               "The message to send to the "
               "target agent session"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"target_session", "message"})}
  };
  tools.push_back(send_to_session_tool);

  // Built-in tool: ingest_document (RAG)
  LlmToolDecl ingest_tool;
  ingest_tool.name = "ingest_document";
  ingest_tool.description =
      "Ingest a document into the knowledge "
      "base for semantic search. The text is "
      "split into chunks, embedded, and "
      "stored in the local vector database.";
  ingest_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"source", {
              {"type", "string"},
              {"description",
               "Source identifier (filename, "
               "URL, or label)"}
          }},
          {"text", {
              {"type", "string"},
              {"description",
               "The document text to ingest"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"source", "text"})}
  };
  tools.push_back(ingest_tool);

  // Built-in tool: search_knowledge (RAG)
  LlmToolDecl search_tool;
  search_tool.name = "search_knowledge";
  search_tool.description =
      "Search the knowledge base using "
      "semantic similarity. Returns the "
      "most relevant document chunks.";
  search_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"query", {
              {"type", "string"},
              {"description",
               "The search query"}
          }},
          {"top_k", {
              {"type", "integer"},
              {"description",
               "Number of results (default 5)"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"query"})}
  };
  tools.push_back(search_tool);

  // Built-in tool: run_supervisor
  LlmToolDecl run_supervisor_tool;
  run_supervisor_tool.name = "run_supervisor";
  run_supervisor_tool.description =
      "Run a supervisor agent that decomposes "
      "a complex goal into sub-tasks and "
      "delegates them to specialized role "
      "agents. Each role agent has its own "
      "system prompt and tool restrictions. "
      "Results are aggregated into a single "
      "response. Requires agent_roles.json "
      "configuration.";
  run_supervisor_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"goal", {
              {"type", "string"},
              {"description",
               "The high-level goal to "
               "decompose and delegate"}
          }},
          {"strategy", {
              {"type", "string"},
              {"enum", nlohmann::json::array(
                  {"sequential", "parallel"})},
              {"description",
               "Execution strategy: "
               "'sequential' (default) or "
               "'parallel'"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"goal"})}
  };
  tools.push_back(run_supervisor_tool);

  // Built-in tool: list_agent_roles
  LlmToolDecl list_roles_tool;
  list_roles_tool.name = "list_agent_roles";
  list_roles_tool.description =
      "List all configured agent roles with "
      "their names, system prompts, and "
      "allowed tools.";
  list_roles_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}
  };
  tools.push_back(list_roles_tool);

  // Built-in tool: create_pipeline
  LlmToolDecl create_pipeline_tool;
  create_pipeline_tool.name =
      "create_pipeline";
  create_pipeline_tool.description =
      "Create a multi-step pipeline for "
      "deterministic workflow execution. "
      "Each step can be a tool call, LLM "
      "prompt, or conditional branch. "
      "Steps execute sequentially, and "
      "output from each step is available "
      "to subsequent steps via "
      "{{variable}} interpolation.";
  create_pipeline_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"name", {
              {"type", "string"},
              {"description",
               "Pipeline name"}
          }},
          {"description", {
              {"type", "string"},
              {"description",
               "Pipeline description"}
          }},
          {"trigger", {
              {"type", "string"},
              {"description",
               "Trigger type: 'manual' or "
               "'cron:daily HH:MM' etc."}
          }},
          {"steps", {
              {"type", "array"},
              {"description",
               "Array of step objects"},
              {"items", {
                  {"type", "object"},
                  {"properties", {
                      {"id", {
                          {"type", "string"},
                          {"description",
                           "Step identifier"}}},
                      {"type", {
                          {"type", "string"},
                          {"description",
                           "Step type: tool, "
                           "prompt, or condition"}}},
                      {"tool_name", {
                          {"type", "string"},
                          {"description",
                           "Tool to invoke"}}},
                      {"args", {
                          {"type", "object"},
                          {"description",
                           "Tool arguments"}}},
                      {"prompt", {
                          {"type", "string"},
                          {"description",
                           "LLM prompt text"}}},
                      {"condition", {
                          {"type", "string"},
                          {"description",
                           "Condition expression"
                          }}},
                      {"then_step", {
                          {"type", "string"},
                          {"description",
                           "Step ID if true"}}},
                      {"else_step", {
                          {"type", "string"},
                          {"description",
                           "Step ID if false"}}},
                      {"output_var", {
                          {"type", "string"},
                          {"description",
                           "Variable name for "
                           "step output"}}},
                      {"skip_on_failure", {
                          {"type", "boolean"},
                          {"description",
                           "Continue on error"
                          }}},
                      {"max_retries", {
                          {"type", "integer"},
                          {"description",
                           "Max retry count"}}}
                  }},
                  {"required",
                   nlohmann::json::array(
                       {"id", "type"})}
              }}
          }}
      }},
      {"required", nlohmann::json::array(
          {"name", "steps"})}
  };
  tools.push_back(create_pipeline_tool);

  // Built-in tool: list_pipelines
  LlmToolDecl list_pipelines_tool;
  list_pipelines_tool.name = "list_pipelines";
  list_pipelines_tool.description =
      "List all configured pipelines with "
      "their names, triggers, and step "
      "counts.";
  list_pipelines_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}
  };
  tools.push_back(list_pipelines_tool);

  // Built-in tool: run_pipeline
  LlmToolDecl run_pipeline_tool;
  run_pipeline_tool.name = "run_pipeline";
  run_pipeline_tool.description =
      "Execute a pipeline by its ID. "
      "Optionally provide input variables "
      "that can be referenced in steps "
      "via {{variable}} syntax.";
  run_pipeline_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"pipeline_id", {
              {"type", "string"},
              {"description",
               "The pipeline ID to execute"}
          }},
          {"input_vars", {
              {"type", "object"},
              {"description",
               "Input variables (key-value "
               "pairs) available to all "
               "pipeline steps"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"pipeline_id"})}
  };
  tools.push_back(run_pipeline_tool);

  // Built-in tool: delete_pipeline
  LlmToolDecl delete_pipeline_tool;
  delete_pipeline_tool.name =
      "delete_pipeline";
  delete_pipeline_tool.description =
      "Delete a pipeline by its ID.";
  delete_pipeline_tool.parameters = {
      {"type", "object"},
      {"properties", {
          {"pipeline_id", {
              {"type", "string"},
              {"description",
               "The pipeline ID to delete"}
          }}
      }},
      {"required", nlohmann::json::array(
          {"pipeline_id"})}
  };
  tools.push_back(delete_pipeline_tool);

#ifdef TIZEN_ACTION_ENABLED
  if (action_bridge_) {
    // Load per-action tools from cached MD files
    auto cached = action_bridge_->LoadCachedActions();
    for (const auto& schema : cached) {
      std::string aname =
          schema.value("name", "");
      if (aname.empty()) continue;

      std::string adesc =
          schema.value("description", "");

      LlmToolDecl tool;
      tool.name = "action_" + aname;
      tool.description = adesc +
          " (Tizen Action: " + aname +
          "). Execute this action on the "
          "device via the Tizen Action "
          "Framework.";

      // Build parameters from inputSchema
      nlohmann::json props =
          nlohmann::json::object();
      nlohmann::json required_arr =
          nlohmann::json::array();

      if (schema.contains("inputSchema") &&
          schema["inputSchema"].contains(
              "properties")) {
        props =
            schema["inputSchema"]["properties"];
        if (schema["inputSchema"].contains(
                "required")) {
          required_arr =
              schema["inputSchema"]["required"];
        }
      }

      tool.parameters = {
          {"type", "object"},
          {"properties", props},
          {"required", required_arr}};
      tools.push_back(tool);
    }

    // Fallback: generic execute_action tool
    LlmToolDecl exec_action_tool;
    exec_action_tool.name = "execute_action";
    exec_action_tool.description =
        "Execute a Tizen Action Framework "
        "action by name with given arguments. "
        "Prefer using action_<name> tools "
        "when available.";
    exec_action_tool.parameters = {
        {"type", "object"},
        {"properties", {
            {"name", {
                {"type", "string"},
                {"description",
                 "The action name to execute"}
            }},
            {"arguments", {
                {"type", "object"},
                {"description",
                 "Arguments for the action"}
            }}
        }},
        {"required",
         nlohmann::json::array(
             {"name"})}};
    tools.push_back(exec_action_tool);
  }
#endif

  cached_tools_ = tools;
  cached_tools_loaded_.store(true);
  return tools;
}

void AgentCore::ReloadSkills() {
  LOG(INFO) << "Reloading skill declarations";
  {
    std::lock_guard<std::mutex> lock(
        tools_mutex_);
    cached_tools_.clear();
  }
  cached_tools_loaded_.store(false);

  // Force reload and rebuild system prompt
  auto tools = LoadSkillDeclarations();
  system_prompt_ =
      BuildSystemPrompt(tools);
  LOG(INFO) << "Skill reload complete: "
            << tools.size() << " tools";
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
  std::string prompt = system_prompt_;

  // Build tool list string
  std::string tool_list;
  for (const auto& t : tools) {
    tool_list += "- " + t.name + ": "
        + t.description + "\n";
  }

  // Load embedded tool descriptions from MD
  {
    namespace fs = std::filesystem;
    const std::string embedded_dir =
        "/opt/usr/share/tizenclaw/tools/embedded";
    std::error_code ec;
    if (fs::exists(embedded_dir, ec)) {
      std::string embedded_docs;
      for (const auto& entry :
           fs::directory_iterator(
               embedded_dir, ec)) {
        if (!entry.is_regular_file()) continue;
        if (entry.path().extension() != ".md")
          continue;
        std::ifstream in(entry.path());
        if (!in.is_open()) continue;
        std::string content(
            (std::istreambuf_iterator<char>(in)),
            std::istreambuf_iterator<char>());
        if (!content.empty()) {
          embedded_docs += "\n" + content + "\n";
        }
      }
      if (!embedded_docs.empty()) {
        tool_list +=
            "\n## Embedded Tool Details\n"
            + embedded_docs;
      }
    }
  }

  // Load action tool descriptions from MD
  {
    namespace fs = std::filesystem;
    const std::string actions_dir =
        "/opt/usr/share/tizenclaw/tools/actions";
    std::error_code ec;
    if (fs::exists(actions_dir, ec)) {
      std::string action_docs;
      for (const auto& entry :
           fs::directory_iterator(
               actions_dir, ec)) {
        if (!entry.is_regular_file()) continue;
        if (entry.path().extension() != ".md")
          continue;
        std::ifstream in(entry.path());
        if (!in.is_open()) continue;
        std::string content(
            (std::istreambuf_iterator<char>(in)),
            std::istreambuf_iterator<char>());
        if (!content.empty()) {
          action_docs += "\n" + content + "\n";
        }
      }
      if (!action_docs.empty()) {
        tool_list +=
            "\n## Device Action Details\n"
            + action_docs;
      }
    }
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

void AgentCore::CompactHistory(
    const std::string& session_id) {
  // MUST be called with session_mutex_ held
  auto& history = sessions_[session_id];
  if (history.size() <= kCompactionThreshold) {
    return;  // Below threshold, no compaction
  }
  if (!backend_) return;

  // Gather oldest N messages to summarize
  size_t count = std::min(
      kCompactionCount, history.size() - 2);
  if (count < 2) return;

  // Extend count to include complete
  // tool_call/tool pairs — never split a pair
  while (count < history.size() - 2) {
    auto& last = history[count - 1];
    if (last.role == "assistant" &&
        !last.tool_calls.empty()) {
      // Include all following tool results
      while (count < history.size() - 2 &&
             history[count].role == "tool") {
        count++;
      }
      break;
    }
    if (last.role == "tool") {
      count++;  // Include this tool in compaction
    } else {
      break;
    }
  }

  std::vector<LlmMessage> to_summarize(
      history.begin(),
      history.begin() + count);

  // Build compaction prompt
  std::string summary_prompt;
  for (auto& msg : to_summarize) {
    summary_prompt += msg.role + ": ";
    if (!msg.text.empty()) {
      summary_prompt += msg.text;
    }
    if (!msg.tool_calls.empty()) {
      summary_prompt += "[called tools: ";
      for (auto& tc : msg.tool_calls) {
        summary_prompt += tc.name + " ";
      }
      summary_prompt += "]";
    }
    if (msg.role == "tool" &&
        !msg.tool_name.empty()) {
      summary_prompt += "[" + msg.tool_name
          + " result]";
    }
    summary_prompt += "\n";
  }

  LlmMessage compact_prompt;
  compact_prompt.role = "user";
  compact_prompt.text =
      "Summarize this conversation concisely "
      "in 2-3 sentences. Preserve key facts, "
      "decisions, and results. Respond ONLY "
      "with the summary, nothing else:\n\n"
      + summary_prompt;

  std::vector<LlmMessage> compact_msgs;
  compact_msgs.push_back(compact_prompt);

  // Release lock temporarily for LLM call
  // (we hold a copy of what we need)
  session_mutex_.unlock();

  LlmResponse resp;
  try {
    resp = backend_->Chat(
        compact_msgs,
        {},       // no tools for compaction
        nullptr,  // no streaming
        "");      // no system prompt
  } catch (...) {
    session_mutex_.lock();
    LOG(WARNING) << "Compaction LLM call failed";
    return;
  }

  session_mutex_.lock();

  // Verify session still exists after re-lock
  if (!sessions_.contains(session_id)) {
    return;
  }
  auto& hist = sessions_[session_id];

  if (!resp.success || resp.text.empty()) {
    LOG(WARNING) << "Compaction failed, "
                 << "falling back to FIFO";
    // Fallback: simple FIFO trim
    while (hist.size() >
           kCompactionThreshold) {
      hist.erase(hist.begin());
    }
    return;
  }

  // Replace oldest N turns with 1 compressed
  LlmMessage compressed;
  compressed.role = "assistant";
  compressed.text =
      "[compressed] " + resp.text;

  hist.erase(
      hist.begin(),
      hist.begin() + count);
  hist.insert(hist.begin(), compressed);

  LOG(INFO) << "Compacted " << count
            << " turns into 1 for session: "
            << session_id;

  // Log compaction token usage if available
  if (resp.total_tokens > 0) {
    session_store_.LogTokenUsage(
        session_id,
        backend_->GetName() + "_compaction",
        resp.prompt_tokens,
        resp.completion_tokens);
  }
}

void AgentCore::TrimHistory(
    const std::string& session_id) {
  // MUST be called with session_mutex_ held
  auto& history = sessions_[session_id];

  // First, try LLM-based compaction
  if (history.size() > kCompactionThreshold) {
    CompactHistory(session_id);
  }

  // Hard limit fallback (FIFO)
  while (history.size() > kMaxHistorySize) {
    history.erase(history.begin());
  }
}

void AgentCore::ClearSession(
    const std::string& session_id) {
  std::lock_guard<std::mutex> lock(session_mutex_);
  sessions_.erase(session_id);
  session_store_.DeleteSession(session_id);
  tool_policy_.ResetSession(session_id);
}

std::string AgentCore::ExecuteSkillForMcp(
    const std::string& skill_name,
    const nlohmann::json& args) {
  return ExecuteSkill(skill_name, args);
}

std::string AgentCore::ExecuteTaskOp(
    const std::string& operation,
    const nlohmann::json& args) {
  if (!scheduler_) {
    return "{\"error\": "
           "\"TaskScheduler not available\"}";
  }

  nlohmann::json result;

  if (operation == "create_task") {
    std::string schedule =
        args.value("schedule", "");
    std::string prompt =
        args.value("prompt", "");
    std::string session_id =
        args.value("session_id", "default");

    if (schedule.empty() || prompt.empty()) {
      result = {
          {"error",
           "schedule and prompt are required"}
      };
    } else {
      std::string task_id =
          scheduler_->CreateTask(
              schedule, prompt, session_id);
      if (task_id.empty()) {
        result = {
            {"error",
             "Invalid schedule expression. "
             "Use: 'daily HH:MM', "
             "'interval Ns/Nm/Nh', "
             "'once YYYY-MM-DD HH:MM', or "
             "'weekly DAY HH:MM'"}
        };
      } else {
        result = {
            {"status", "created"},
            {"task_id", task_id},
            {"schedule", schedule},
            {"prompt", prompt}
        };
      }
    }
  } else if (operation == "list_tasks") {
    std::string session_id =
        args.value("session_id", "");
    auto tasks =
        scheduler_->ListTasks(session_id);
    result = {
        {"status", "ok"},
        {"tasks", tasks},
        {"count", tasks.size()}
    };
  } else if (operation == "cancel_task") {
    std::string task_id =
        args.value("task_id", "");
    if (task_id.empty()) {
      result = {
          {"error", "task_id is required"}
      };
    } else {
      bool ok =
          scheduler_->CancelTask(task_id);
      if (ok) {
        result = {
            {"status", "cancelled"},
            {"task_id", task_id}
        };
      } else {
        result = {
            {"error", "Task not found"},
            {"task_id", task_id}
        };
      }
    }
  } else {
    result = {
        {"error", "Unknown task operation: "
                  + operation}
    };
  }

  return result.dump();
}

LlmResponse AgentCore::TryFallbackBackends(
    const std::vector<LlmMessage>& history,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(
        const std::string&)> on_chunk,
    const std::string& system_prompt) {
  LlmResponse last_resp;
  last_resp.success = false;
  last_resp.error_message =
      "No fallback backends configured";

  if (fallback_names_.empty()) {
    return last_resp;
  }

  for (const auto& fb_name : fallback_names_) {
    LOG(INFO) << "Trying fallback backend: "
              << fb_name;

    auto fb_backend =
        LlmBackendFactory::Create(fb_name);
    if (!fb_backend) {
      LOG(WARNING)
          << "Failed to create fallback: "
          << fb_name;
      continue;
    }

    // Get backend config
    nlohmann::json fb_config;
    if (llm_config_.contains("backends") &&
        llm_config_["backends"]
            .contains(fb_name)) {
      fb_config =
          llm_config_["backends"][fb_name];
    }

    // Decrypt API key if encrypted
    if (fb_config.contains("api_key")) {
      std::string api_key =
          fb_config["api_key"]
              .get<std::string>();
      if (KeyStore::IsEncrypted(api_key)) {
        std::string decrypted =
            KeyStore::Decrypt(api_key);
        if (!decrypted.empty()) {
          fb_config["api_key"] = decrypted;
        }
      }
    }

    // xAI identity injection
    if (fb_name == "xai" ||
        fb_name == "grok") {
      fb_config["provider_name"] = "xai";
      if (!fb_config.contains("endpoint")) {
        fb_config["endpoint"] =
            "https://api.x.ai/v1";
      }
    }

    if (!fb_backend->Initialize(fb_config)) {
      LOG(WARNING)
          << "Failed to init fallback: "
          << fb_name;
      continue;
    }

    // Rate-limit backoff: try with delay
    int backoff_ms = 0;
    if (last_resp.http_status == 429) {
      backoff_ms = 1000;  // 1s initial backoff
      LOG(INFO)
          << "Rate-limited, backing off "
          << backoff_ms << "ms";
      std::this_thread::sleep_for(
          std::chrono::milliseconds(
              backoff_ms));
    }

    LlmResponse resp = fb_backend->Chat(
        history, tools, on_chunk,
        system_prompt);

    if (resp.success) {
      LOG(INFO)
          << "Fallback succeeded: "
          << fb_name;

      // Switch primary backend
      backend_ = std::move(fb_backend);

      AuditLogger::Instance().Log(
          AuditLogger::MakeEvent(
              AuditEventType::kConfigChange,
              "",
              {{"fallback_from",
                "primary"},
               {"fallback_to",
                fb_name}}));

      return resp;
    }

    last_resp = resp;
    LOG(WARNING)
        << "Fallback failed (" << fb_name
        << "): " << resp.error_message;
  }

  return last_resp;
}

std::string AgentCore::ExecuteSessionOp(
    const std::string& operation,
    const nlohmann::json& args,
    const std::string& caller_session) {
  LOG(INFO) << "SessionOp: " << operation;

  if (operation == "create_session") {
    std::string name =
        args.value("name", "agent");
    std::string prompt =
        args.value("system_prompt", "");

    if (prompt.empty()) {
      return "{\"error\": \"system_prompt "
             "is required\"}";
    }

    // Generate unique session_id
    auto now = std::chrono::system_clock::now();
    auto ts = std::chrono::duration_cast<
        std::chrono::milliseconds>(
        now.time_since_epoch()).count();
    std::string session_id =
        "agent_" + name + "_" +
        std::to_string(ts % 100000);

    // Store per-session system prompt
    {
      std::lock_guard<std::mutex> lock(
          session_mutex_);
      session_prompts_[session_id] = prompt;
    }

    LOG(INFO) << "Created agent session: "
              << session_id;

    nlohmann::json result = {
        {"status", "ok"},
        {"session_id", session_id},
        {"name", name},
        {"system_prompt_length",
         (int)prompt.size()}
    };
    return result.dump();
  }

  if (operation == "list_sessions") {
    nlohmann::json sessions =
        nlohmann::json::array();
    {
      std::lock_guard<std::mutex> lock(
          session_mutex_);
      for (auto& [sid, prompt] :
           session_prompts_) {
        nlohmann::json s = {
            {"session_id", sid},
            {"system_prompt",
             prompt.substr(
                 0, std::min((size_t)100,
                             prompt.size()))
             + (prompt.size() > 100
                    ? "..."
                    : "")},
            {"history_size",
             sessions_.contains(sid)
                 ? (int)sessions_[sid].size()
                 : 0}
        };
        sessions.push_back(s);
      }

      // Also list sessions without custom
      // prompts (default sessions)
      for (auto& [sid, hist] : sessions_) {
        if (!session_prompts_.contains(sid)) {
          nlohmann::json s = {
              {"session_id", sid},
              {"system_prompt", "(default)"},
              {"history_size",
               (int)hist.size()}
          };
          sessions.push_back(s);
        }
      }
    }

    nlohmann::json result = {
        {"status", "ok"},
        {"sessions", sessions},
        {"count", (int)sessions.size()}
    };
    return result.dump();
  }

  if (operation == "send_to_session") {
    std::string target =
        args.value("target_session", "");
    std::string message =
        args.value("message", "");

    if (target.empty() || message.empty()) {
      return "{\"error\": \"target_session "
             "and message are required\"}";
    }

    // Prevent self-messaging loop
    if (target == caller_session) {
      return "{\"error\": \"Cannot send "
             "message to self\"}";
    }

    LOG(INFO) << "Sending to session: "
              << target << " from: "
              << caller_session;

    // Call ProcessPrompt on target session
    // Note: no streaming for inter-agent
    std::string response =
        ProcessPrompt(target, message);

    nlohmann::json result = {
        {"status", "ok"},
        {"target_session", target},
        {"response", response}
    };
    return result.dump();
  }

  return "{\"error\": \"Unknown session "
         "operation: " + operation + "\"}";
}

std::string AgentCore::GetSessionPrompt(
    const std::string& session_id,
    const std::vector<LlmToolDecl>& tools) {
  // Check for per-session prompt override
  {
    std::lock_guard<std::mutex> lock(
        session_mutex_);
    auto it = session_prompts_.find(session_id);
    if (it != session_prompts_.end()) {
      // Use custom prompt with tool list
      std::string prompt = it->second;

      std::string tool_list;
      for (const auto& t : tools) {
        tool_list += "- " + t.name + ": "
            + t.description + "\n";
      }

      const std::string placeholder =
          "{{AVAILABLE_TOOLS}}";
      size_t pos = prompt.find(placeholder);
      if (pos != std::string::npos) {
        prompt.replace(
            pos, placeholder.size(),
            tool_list);
      } else if (!tool_list.empty()) {
        prompt +=
            "\n\nAvailable tools:\n" + tool_list;
      }

      return prompt;
    }
  }

  // Fallback to global system prompt
  return BuildSystemPrompt(tools);
}

std::string AgentCore::ExecuteRagOp(
    const std::string& operation,
    const nlohmann::json& args) {
  if (operation == "ingest_document") {
    std::string source =
        args.value("source", "");
    std::string text =
        args.value("text", "");

    if (source.empty() || text.empty()) {
      return "{\"error\": \"source and text "
             "are required\"}";
    }

    auto chunks =
        EmbeddingStore::ChunkText(text);
    int stored = 0;
    for (const auto& chunk : chunks) {
      auto emb = GenerateEmbedding(chunk);
      if (emb.empty()) {
        LOG(WARNING)
            << "Failed to generate embedding "
            << "for chunk (" << chunk.size()
            << " chars)";
        continue;
      }
      if (embedding_store_.StoreChunk(
              source, chunk, emb)) {
        stored++;
      }
    }

    nlohmann::json result = {
        {"status", "ok"},
        {"source", source},
        {"chunks_total",
         static_cast<int>(chunks.size())},
        {"chunks_stored", stored},
        {"total_documents",
         embedding_store_.GetChunkCount()}
    };
    return result.dump();
  }

  if (operation == "search_knowledge") {
    std::string query =
        args.value("query", "");
    int top_k = args.value("top_k", 5);

    if (query.empty()) {
      return "{\"error\": \"query is required\"}";
    }

    auto query_emb = GenerateEmbedding(query);
    if (query_emb.empty()) {
      return "{\"error\": \"Failed to generate "
             "query embedding\"}";
    }

    auto results =
        embedding_store_.Search(query_emb, top_k);

    nlohmann::json j_results =
        nlohmann::json::array();
    for (const auto& r : results) {
      j_results.push_back({
          {"source", r.source},
          {"text", r.chunk_text},
          {"score", r.score}
      });
    }

    return nlohmann::json({
        {"status", "ok"},
        {"query", query},
        {"results", j_results}
    }).dump();
  }

  return "{\"error\": \"Unknown RAG operation\"}";
}

std::vector<float> AgentCore::GenerateEmbedding(
    const std::string& text) {
  if (!backend_ || text.empty()) return {};

  std::string backend_name =
      backend_->GetName();

  // Determine embedding API endpoint + model
  std::string url;
  std::string model;
  std::string api_key;

  // Get backend config
  nlohmann::json bc;
  if (llm_config_.contains("backends") &&
      llm_config_["backends"].contains(
          backend_name)) {
    bc = llm_config_["backends"][backend_name];
  }
  api_key = bc.value("api_key", "");
  if (KeyStore::IsEncrypted(api_key)) {
    api_key = KeyStore::Decrypt(api_key);
  }

  nlohmann::json req_body;
  std::map<std::string, std::string> headers;

  if (backend_name == "gemini") {
    model = bc.value(
        "embedding_model",
        "text-embedding-004");
    url = "https://generativelanguage.googleapis"
          ".com/v1beta/models/" + model +
          ":embedContent?key=" + api_key;
    req_body = {
        {"model", "models/" + model},
        {"content", {{"parts",
            {{{"text", text}}}
        }}}
    };
  } else if (backend_name == "openai" ||
             backend_name == "xai" ||
             backend_name == "grok") {
    std::string endpoint = bc.value(
        "endpoint",
        "https://api.openai.com/v1");
    model = bc.value(
        "embedding_model",
        "text-embedding-3-small");
    url = endpoint + "/embeddings";
    req_body = {
        {"model", model},
        {"input", text}
    };
    headers = {
        {"Authorization",
         "Bearer " + api_key},
        {"Content-Type", "application/json"}
    };
  } else if (backend_name == "ollama") {
    std::string endpoint = bc.value(
        "endpoint",
        "http://localhost:11434");
    model = bc.value(
        "embedding_model", "nomic-embed-text");
    url = endpoint + "/api/embeddings";
    req_body = {
        {"model", model},
        {"prompt", text}
    };
    headers = {
        {"Content-Type", "application/json"}
    };
  } else {
    LOG(WARNING) << "No embedding support for: "
                 << backend_name;
    return {};
  }

  if (backend_name == "gemini") {
    headers = {
        {"Content-Type", "application/json"}
    };
  }

  auto resp = HttpClient::Post(
      url, headers, req_body.dump(), 2);

  if (!resp.success) {
    LOG(ERROR) << "Embedding API failed: "
               << resp.error;
    return {};
  }

  try {
    auto j = nlohmann::json::parse(resp.body);
    std::vector<float> emb;

    if (backend_name == "gemini") {
      // Response: {"embedding":{"values":[...]}}
      if (j.contains("embedding") &&
          j["embedding"].contains("values")) {
        for (auto& v :
             j["embedding"]["values"]) {
          emb.push_back(v.get<float>());
        }
      }
    } else if (backend_name == "openai" ||
               backend_name == "xai" ||
               backend_name == "grok") {
      // Response: {"data":[{"embedding":[...]}]}
      if (j.contains("data") &&
          !j["data"].empty() &&
          j["data"][0].contains("embedding")) {
        for (auto& v :
             j["data"][0]["embedding"]) {
          emb.push_back(v.get<float>());
        }
      }
    } else if (backend_name == "ollama") {
      // Response: {"embedding":[...]}
      if (j.contains("embedding")) {
        for (auto& v : j["embedding"]) {
          emb.push_back(v.get<float>());
        }
      }
    }

    if (emb.empty()) {
      LOG(WARNING) << "Empty embedding from: "
                   << backend_name;
    }
    return emb;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Embedding parse error: "
               << e.what();
    return {};
  }
}

std::string AgentCore::ExecuteSupervisorOp(
    const std::string& operation,
    const nlohmann::json& args,
    const std::string& session_id) {
  if (!supervisor_) {
    return "{\"error\": "
           "\"SupervisorEngine not available\"}";
  }

  nlohmann::json result;

  if (operation == "run_supervisor") {
    std::string goal =
        args.value("goal", "");
    std::string strategy =
        args.value("strategy", "sequential");

    if (goal.empty()) {
      result = {
          {"error", "goal is required"}
      };
    } else {
      std::string response =
          supervisor_->RunSupervisor(
              goal, strategy, session_id);
      result = {
          {"status", "ok"},
          {"goal", goal},
          {"strategy", strategy},
          {"result", response}
      };
    }
  } else if (operation == "list_agent_roles") {
    auto roles = supervisor_->ListRoles();
    result = {
        {"status", "ok"},
        {"roles", roles},
        {"count", (int)roles.size()}
    };
  } else {
    result = {
        {"error",
         "Unknown supervisor operation: "
         + operation}
    };
  }

  return result.dump();
}

std::vector<LlmToolDecl>
AgentCore::GetToolsFiltered(
    const std::vector<std::string>& allowed) {
  auto all_tools = LoadSkillDeclarations();

  // Empty allowed list = all tools
  if (allowed.empty()) {
    return all_tools;
  }

  std::vector<LlmToolDecl> filtered;
  for (auto& tool : all_tools) {
    for (auto& name : allowed) {
      if (tool.name == name) {
        filtered.push_back(tool);
        break;
      }
    }
  }

  return filtered;
}

std::string AgentCore::ExecutePipelineOp(
    const std::string& operation,
    const nlohmann::json& args,
    const std::string& session_id) {
  (void)session_id;  // Reserved for future use
  if (!pipeline_executor_) {
    return "{\"error\": "
           "\"PipelineExecutor not available\"}";
  }

  nlohmann::json result;

  if (operation == "create_pipeline") {
    std::string id =
        pipeline_executor_->CreatePipeline(args);
    if (id.empty()) {
      result = {
          {"error",
           "Failed to create pipeline. "
           "name and steps are required."}
      };
    } else {
      result = {
          {"status", "ok"},
          {"pipeline_id", id},
          {"name", args.value("name", "")}
      };
    }
  } else if (operation == "list_pipelines") {
    auto pipelines =
        pipeline_executor_->ListPipelines();
    result = {
        {"status", "ok"},
        {"pipelines", pipelines},
        {"count",
         static_cast<int>(pipelines.size())}
    };
  } else if (operation == "run_pipeline") {
    std::string pid =
        args.value("pipeline_id", "");
    if (pid.empty()) {
      result = {
          {"error", "pipeline_id is required"}
      };
    } else {
      nlohmann::json input_vars =
          args.value("input_vars",
                     nlohmann::json::object());
      auto run_result =
          pipeline_executor_->RunPipeline(
              pid, input_vars);

      nlohmann::json steps_json =
          nlohmann::json::array();
      for (auto& [step_id, step_result] :
           run_result.step_results) {
        steps_json.push_back({
            {"step_id", step_id},
            {"result",
             step_result.substr(
                 0,
                 std::min((size_t)500,
                          step_result.size()))}
        });
      }

      result = {
          {"status", run_result.status},
          {"pipeline_id",
           run_result.pipeline_id},
          {"duration_ms",
           run_result.duration_ms},
          {"steps", steps_json}
      };
    }
  } else if (operation == "delete_pipeline") {
    std::string pid =
        args.value("pipeline_id", "");
    if (pid.empty()) {
      result = {
          {"error", "pipeline_id is required"}
      };
    } else {
      bool ok =
          pipeline_executor_->DeletePipeline(
              pid);
      if (ok) {
        result = {
            {"status", "ok"},
            {"deleted", pid}
        };
      } else {
        result = {
            {"error",
             "Pipeline not found: " + pid}
        };
      }
    }
  } else {
    result = {
        {"error",
         "Unknown pipeline operation: "
         + operation}
    };
  }

  return result.dump();
}

std::string AgentCore::ExecuteActionOp(
    const std::string& operation,
    const nlohmann::json& args) {
#ifdef TIZEN_ACTION_ENABLED
  if (!action_bridge_) {
    return "{\"error\":"
           "\"Action bridge not available\"}";
  }

  if (operation == "execute_action") {
    std::string name =
        args.value("name", "");
    nlohmann::json arguments =
        args.value("arguments",
                   nlohmann::json::object());
    if (name.empty()) {
      return "{\"error\":"
             "\"Action name is required\"}";
    }
    LOG(INFO)
        << "Executing Tizen action: " << name;
    return action_bridge_->ExecuteAction(
        name, arguments);
  }

  // Per-action tool: action_<name>
  if (operation.starts_with("action_")) {
    std::string name =
        operation.substr(7);  // skip "action_"
    LOG(INFO)
        << "Executing Tizen action (tool): "
        << name;
    return action_bridge_->ExecuteAction(
        name, args);
  }

  return "{\"error\":\"Unknown action op: "
         + operation + "\"}";
#else
  (void)operation;
  (void)args;
  return "{\"error\":"
         "\"Tizen Action not supported "
         "in this build\"}";
#endif
}

} // namespace tizenclaw
