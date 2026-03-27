/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "agent_core.hh"

#include <aul.h>
#include <bundle.h>
#include <curl/curl.h>
#include <malloc.h>
#include <sqlite3.h>
#include <sys/stat.h>

#include <algorithm>
#include <chrono>
#include <climits>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <thread>

#include "../../common/boot_status_logger.hh"
#include "../../common/logging.hh"
#include "../infra/http_client.hh"
#include "../infra/key_store.hh"
#include "../llm/plugin_llm_backend.hh"
#include "../llm/plugin_manager.hh"
#include "../storage/audit_logger.hh"
#include "cli_plugin_manager.hh"
#include "skill_plugin_manager.hh"
#include "system_cli_adapter.hh"
#include "capability_registry.hh"
#include "skill_manifest.hh"
#include "skill_verifier.hh"
#include "tool_indexer.hh"
#include "tool_declaration_builder.hh"

namespace tizenclaw {

AgentCore::AgentCore()
    : container_(std::make_unique<ContainerEngine>()), initialized_(false) {}

AgentCore::~AgentCore() {
  Shutdown();
  stop_maintenance_ = true;
  if (maintenance_thread_.joinable()) {
    maintenance_thread_.join();
  }
}

void AgentCore::UpdateActivityTime() {
  auto now = std::chrono::duration_cast<std::chrono::seconds>(
                 std::chrono::system_clock::now().time_since_epoch())
                 .count();
  last_activity_time_.store(now);
}

void AgentCore::MaintenanceLoop() {
  LOG(INFO) << "AgentCore Maintenance thread started";
  // 5 minutes idle timeout
  const int64_t IDLE_TIMEOUT_SEC = 300;

  while (!stop_maintenance_) {
    for (int i = 0; i < 100 && !stop_maintenance_; ++i) {
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    if (stop_maintenance_) break;

    try {
      auto now = std::chrono::duration_cast<std::chrono::seconds>(
                     std::chrono::system_clock::now().time_since_epoch())
                     .count();
      auto last_active = last_activity_time_.load();

      if (last_active > 0 && (now - last_active) > IDLE_TIMEOUT_SEC) {
        LOG(INFO) << "System idle for " << (now - last_active)
                  << "s, flushing memory...";

        // Release SQLite memory cache
        int bytes_freed =
            sqlite3_release_memory(1024 * 1024 * 50);  // Try to free 50MB
        if (bytes_freed > 0) {
          LOG(INFO) << "sqlite3_release_memory freed " << bytes_freed << " bytes";
        }

        // Detach knowledge DBs to reclaim file cache
        embedding_store_.DetachKnowledgeDBs();

        // Return heap memory to OS
        int trim_result = malloc_trim(0);
        LOG(INFO) << "malloc_trim(0) returned " << trim_result;

        // Reset activity time so we don't spam flush
        last_activity_time_.store(0);

        // Regenerate memory summary if dirty
        if (memory_store_.IsSummaryDirty()) {
          memory_store_.RegenerateSummary();
          LOG(INFO) << "Memory summary regenerated "
                    << "(idle)";
        }

        // Prune old memory entries
        memory_store_.PruneShortTerm();
        memory_store_.PruneEpisodic();

        // Free ONNX Runtime memory embedding if loaded (lazy un-load)
        {
          std::lock_guard<std::mutex> lock(embedding_mutex_);
          if (on_device_embedding_.IsAvailable()) {
            on_device_embedding_.Shutdown();
            LOG(INFO) << "On-device embedding model unloaded to save memory (idle)";
          }
        }
      }
    } catch (const std::exception& e) {
      LOG(ERROR) << "MaintenanceLoop exception: "
                 << e.what();
    } catch (...) {
      LOG(ERROR) << "MaintenanceLoop unknown exception";
    }
  }
}

bool AgentCore::Initialize() {
  if (initialized_) return true;

  LOG(INFO) << "AgentCore Initializing...";
  auto& boot = BootStatusLogger::GetInstance();

  {
    auto guard = boot.Track("ContainerEngine");
    if (!container_->Initialize()) {
      guard.SetFailed("initialization error");
      return false;
    }
  }

  // Load LLM config
  {
    auto guard = boot.Track("LlmConfig");
    const char* env_path =
        std::getenv("TIZENCLAW_CONFIG_PATH");
    std::string config_path =
        env_path
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
        LOG(ERROR) << "Failed to parse "
                   << config_path << ": "
                   << e.what();
      }
    }

    // Fallback: try legacy gemini_api_key.txt
    if (llm_config.empty()) {
      LOG(WARNING)
          << "llm_config.json not found, "
          << "using legacy gemini key file";
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
          {"backends",
           {{"gemini",
             {{"api_key", api_key},
              {"model",
               "gemini-2.5-flash"}}}}}};
    }

    llm_config_ = llm_config;
    curl_global_init(CURL_GLOBAL_DEFAULT);
  }

  // Load system prompt
  {
    auto guard = boot.Track("SystemPrompt");
    system_prompt_ =
        LoadSystemPrompt(llm_config_);
    if (!system_prompt_.empty()) {
      LOG(INFO) << "System prompt loaded ("
                << system_prompt_.size()
                << " chars)";
    } else {
      LOG(WARNING)
          << "No system prompt configured";
    }

    // Load condensed web API catalog
    web_api_catalog_ = LoadWebApiCatalog();
    if (!web_api_catalog_.empty()) {
      LOG(INFO) << "Web API catalog loaded ("
                << web_api_catalog_.size()
                << " chars)";
    }
  }

  // Load external MCP servers configuration
  {
    auto guard = boot.Track("McpServers");
    mcp_client_manager_ = std::make_unique<McpClientManager>();
    if (!mcp_client_manager_->LoadConfigAndConnect(
            "/opt/usr/share/tizenclaw/config/"
            "mcp_servers.json")) {
      LOG(INFO) << "No MCP clients configured or config missing.";
    }
  }

  // Load tool execution policy
  {
    auto guard = boot.Track("ToolPolicy");
    std::string policy_path =
        "/opt/usr/share/tizenclaw/config/"
        "tool_policy.json";
    if (!tool_policy_.LoadConfig(policy_path)) {
      LOG(WARNING)
          << "Tool policy config not loaded"
          << " (using defaults)";
    }

    // Initialize ToolRouter with policy aliases
    const auto& aliases =
        tool_policy_.GetAliases();
    if (!aliases.empty()) {
      nlohmann::json alias_json(aliases);
      tool_router_.LoadAliases(alias_json);
    }
  }

  // Load safety guard (physical bounds + device profile)
  {
    auto guard = boot.Track("SafetyGuard");
    std::string bounds_path =
        std::string(APP_DATA_DIR) +
        "/config/safety_bounds.json";
    safety_guard_.LoadConfig(bounds_path);

    std::string profile_path =
        std::string(APP_DATA_DIR) +
        "/config/device_profile.json";
    safety_guard_.LoadDeviceProfile(profile_path);
  }

  // Load offline fallback rules
  {
    auto guard = boot.Track("OfflineFallback");
    std::string fallback_path =
        std::string(APP_DATA_DIR) +
        "/config/offline_fallback.json";
    offline_fallback_.LoadConfig(fallback_path);
  }

  // Load user profiles
  {
    auto guard = boot.Track("UserProfile");
    std::string profile_db_path =
        std::string(APP_DATA_DIR) +
        "/config/user_profiles.json";
    user_profile_store_.Initialize(profile_db_path);
  }

  // Initialize Swarm Networking
  {
    auto guard = boot.Track("SwarmManager");
    swarm_manager_ = std::make_unique<SwarmManager>(safety_guard_);
    if (!swarm_manager_->Start()) {
      LOG(WARNING) << "Failed to start SwarmManager, multi-device networking disabled.";
    }
  }

  {
    auto guard = boot.Track("LlmBackend");
    if (!SwitchToBestBackend(false)) {
      guard.SetFailed(
          "no working backend found");
      return false;
    }

    // Audit: config loaded
    AuditLogger::Instance().Log(
        AuditLogger::MakeEvent(
            AuditEventType::kConfigChange, "",
            {{"backend",
              backend_->GetName()}}));

    LOG(INFO) << "AgentCore initialized with "
              << "backend: "
              << backend_->GetName();
    initialized_ = true;
  }

  // React to plugin install/uninstall
  PluginManager::GetInstance()
      .SetChangeCallback(
          [this]() { ReloadBackend(); });

  // React to skill RPK install/uninstall
  {
    auto guard =
        boot.Track("SkillPluginManager");
    SkillPluginManager::GetInstance()
        .SetChangeCallback([this]() {
          LOG(INFO)
              << "Skill RPK change detected,"
              << " invalidating tool cache";
          cached_tools_loaded_.store(false);
        });
    SkillPluginManager::GetInstance()
        .Initialize();
  }

  // React to CLI TPK install/uninstall
  {
    auto guard =
        boot.Track("CliPluginManager");
    CliPluginManager::GetInstance()
        .SetChangeCallback([this]() {
          LOG(INFO)
              << "CLI TPK change detected,"
              << " invalidating tool cache";
          cached_tools_loaded_.store(false);
        });
    CliPluginManager::GetInstance()
        .Initialize();
  }

  // Initialize system CLI adapter
  {
    auto guard =
        boot.Track("SystemCliAdapter");
    SystemCliAdapter::GetInstance().Initialize(
        std::string(APP_DATA_DIR) +
        "/config/system_cli_config.json");
  }

  // Initialize embedding store for RAG
  {
    auto guard =
        boot.Track("EmbeddingStore");
    std::string rag_db =
        std::string(APP_DATA_DIR) +
        "/rag/embeddings.db";
    std::string rag_dir =
        std::string(APP_DATA_DIR) + "/rag";
    mkdir(rag_dir.c_str(), 0755);
    if (embedding_store_.Initialize(rag_db)) {
      LOG(INFO)
          << "RAG embedding store ready";

      // Scan rag/ directory for .db files
      namespace fs = std::filesystem;
      std::error_code ec;
      if (fs::is_directory(rag_dir, ec)) {
        for (const auto& entry :
             fs::directory_iterator(
                 rag_dir, ec)) {
          if (!entry.is_regular_file())
            continue;
          auto fname =
              entry.path().filename().string();
          if (entry.path().extension() !=
              ".db")
            continue;
          if (fname == "embeddings.db")
            continue;
          embedding_store_
                  .RegisterKnowledgeDB(
                      entry.path()
                          .string());
          LOG(INFO)
              << "Knowledge DB registered: "
              << fname;
        }
      }
      LOG(INFO)
          << "Knowledge DBs registered: "
          << embedding_store_
                 .GetPendingKnowledgeCount()
          << " (lazy, will attach on first search)";
    } else {
      guard.SetFailed("init failed");
      LOG(WARNING)
          << "RAG embedding store "
          << "init failed (non-fatal)";
    }
  }

  // Removed pre-loading of OnDeviceEmbedding. It will be loaded lazily on first use.

  // Initialize supervisor engine
  {
    auto guard =
        boot.Track("SupervisorEngine");
    supervisor_ =
        std::make_unique<SupervisorEngine>(
            this);
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
  }

  // Initialize system context provider
  {
    auto guard =
        boot.Track("SystemContextProvider");
    system_context_ =
        std::make_unique<
            SystemContextProvider>();
    system_context_->Start();
    LOG(INFO)
        << "SystemContextProvider ready";
  }

  // Initialize agent factory
  {
    auto guard = boot.Track("AgentFactory");
    agent_factory_ =
        std::make_unique<AgentFactory>(
            this, supervisor_.get());
    LOG(INFO) << "AgentFactory ready";
  }

  // Initialize auto skill generation agent
  {
    auto guard =
        boot.Track("AutoSkillAgent");
    auto_skill_agent_ =
        std::make_unique<AutoSkillAgent>(this);
    LOG(INFO) << "AutoSkillAgent ready";
  }

  // Initialize pipeline executor
  {
    auto guard =
        boot.Track("PipelineExecutor");
    pipeline_executor_ =
        std::make_unique<PipelineExecutor>(
            this);
    pipeline_executor_->LoadPipelines();
    LOG(INFO) << "Pipeline executor ready";
  }

  // Initialize workflow engine
  {
    auto guard =
        boot.Track("WorkflowEngine");
    workflow_engine_ =
        std::make_unique<WorkflowEngine>(this);
    workflow_engine_->LoadWorkflows();
    LOG(INFO) << "Workflow engine ready";
  }

  // Initialize Tizen Action Framework bridge
  {
    auto guard = boot.Track("ActionBridge");
    action_bridge_ =
        std::make_unique<ActionBridge>();
    if (action_bridge_->Start()) {
      action_bridge_->SyncActionSchemas();
      action_bridge_->SetChangeCallback(
          [this]() {
            LOG(INFO)
                << "Action schemas changed,"
                << " reloading tools";
            cached_tools_loaded_.store(false);
          });
      LOG(INFO)
          << "Tizen Action bridge ready";
    } else {
      guard.SetFailed(
          "init failed (non-fatal)");
      LOG(WARNING)
          << "Tizen Action bridge "
          << "init failed (non-fatal)";
      action_bridge_.reset();
    }
  }

  // Initialize Canvas IPC Server
  {
    auto guard = boot.Track("CanvasIpcServer");
    canvas_ipc_server_ = std::make_unique<CanvasIpcServer>();
    if (!canvas_ipc_server_->Start()) {
      LOG(WARNING) << "Failed to start Canvas IPC server. NUI Canvas integration may not work.";
    } else {
      LOG(INFO) << "Canvas IPC Server ready";
    }
  }

  // Initialize memory store
  {
    auto guard = boot.Track("MemoryStore");
    std::string mem_config_path =
        std::string(APP_DATA_DIR) +
        "/config/memory_config.json";
    memory_store_.LoadConfig(mem_config_path);
    LOG(INFO) << "MemoryStore initialized";
  }

  // Initialize modular tool dispatcher
  {
    auto guard =
        boot.Track("ToolDispatcher");
    tool_dispatcher_ =
        std::make_unique<ToolDispatcher>();
    InitializeToolDispatcher();
  }

  // Start background maintenance thread
  UpdateActivityTime();
  maintenance_thread_ =
      std::thread(
          &AgentCore::MaintenanceLoop, this);

  return true;
}

void AgentCore::Shutdown() {
  if (!initialized_) return;

  LOG(INFO) << "AgentCore Shutting down...";

  // Save all sessions before shutting down
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    for (auto& [sid, history] : sessions_) {
      (void)session_store_.SaveSession(sid, history);
    }
    sessions_.clear();
  }
  {
    std::lock_guard<std::mutex> lock(backend_mutex_);
    if (backend_) {
      backend_->Shutdown();
      backend_.reset();
    }
  }

  if (action_bridge_) {
    action_bridge_->Stop();
    action_bridge_.reset();
  }

  if (canvas_ipc_server_) {
    canvas_ipc_server_->Stop();
    canvas_ipc_server_.reset();
  }

  embedding_store_.Close();
  container_.reset();
  curl_global_cleanup();

  initialized_ = false;
}

std::string AgentCore::ProcessPrompt(
    const std::string& session_id, const std::string& prompt,
    std::function<void(const std::string&)> on_chunk) {
  struct DurationLogger {
    std::string session;
    std::chrono::steady_clock::time_point start;
    explicit DurationLogger(const std::string& s) 
        : session(s), start(std::chrono::steady_clock::now()) {}
    ~DurationLogger() {
      auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
          std::chrono::steady_clock::now() - start).count();
      LOG(INFO) << "Prompt processing for session [" << session << "] took " << elapsed << " ms";
    }
  };
  DurationLogger dlogger(session_id);

  std::shared_ptr<LlmBackend> current_backend;
  {
    std::lock_guard<std::mutex> lock(backend_mutex_);
    current_backend = backend_;
  }

  if (!initialized_ || !current_backend) {
    LOG(ERROR) << "AgentCore not initialized.";
    return "Error: AgentCore is not initialized.";
  }

  LOG(INFO) << "ProcessPrompt [" << session_id << "]: " << prompt;

  UpdateActivityTime();

  // Prompt injection guard: detect known jailbreak patterns
  {
    static const std::vector<std::string> kInjectionPatterns = {
        "ignore previous instructions",
        "ignore all previous",
        "disregard your instructions",
        "forget your instructions",
        "you are now",
        "pretend you are",
        "act as if you have no restrictions",
        "override your system prompt",
        "bypass your safety",
        "DAN mode",
        "jailbreak",
        "이전 지시를 무시",
        "시스템 프롬프트를 무시",
        "제한을 해제",
    };

    std::string lower_prompt = prompt;
    std::transform(lower_prompt.begin(), lower_prompt.end(),
                   lower_prompt.begin(), ::tolower);
    for (const auto& pattern : kInjectionPatterns) {
      std::string lower_pat = pattern;
      std::transform(lower_pat.begin(), lower_pat.end(),
                     lower_pat.begin(), ::tolower);
      if (lower_prompt.find(lower_pat) != std::string::npos) {
        LOG(WARNING) << "Prompt injection detected: \""
                     << pattern << "\" in session: " << session_id;
        AuditLogger::Instance().Log(AuditLogger::MakeEvent(
            AuditEventType::kToolBlocked, session_id,
            {{"reason", "prompt_injection_detected"},
             {"pattern", pattern}}));
        break;
      }
    }
  }

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
  int max_iter = tool_policy_.GetMaxIterations();

  // Reset idle tracking for this prompt
  tool_policy_.ResetIdleTracking(session_id);

  if (canvas_ipc_server_) {
    canvas_ipc_server_->BroadcastState("thinking", "Agent is thinking...");
  }

  while (iterations < max_iter) {
    // Build session-specific system prompt
    std::string full_prompt = GetSessionPrompt(session_id, tools);

    // Query LLM backend with retry (max 3 attempts)
    LlmResponse resp;
    static constexpr int kMaxLlmRetries = 1;
    static constexpr int kLlmRetryBaseMs = 500;
    for (int llm_attempt = 0; llm_attempt < kMaxLlmRetries; ++llm_attempt) {
      resp = current_backend->Chat(
          local_history, tools, on_chunk, full_prompt);
      if (resp.success) break;

      LOG(WARNING) << "LLM attempt " << (llm_attempt + 1)
                   << "/" << kMaxLlmRetries
                   << " failed: " << resp.error_message;

      if (llm_attempt + 1 < kMaxLlmRetries) {
        int delay_ms = kLlmRetryBaseMs * (1 << llm_attempt);
        std::this_thread::sleep_for(
            std::chrono::milliseconds(delay_ms));
      }
    }

    // Track LLM call in health metrics
    if (health_monitor_) health_monitor_->IncrementLlmCallCount();

    if (!resp.success) {
      LOG(ERROR) << "LLM error after retries: " << resp.error_message;

      // Try fallback backends
      resp = TryFallbackBackends(local_history, tools, on_chunk, full_prompt);

      if (!resp.success) {
        // All backends failed — try offline fallback
        auto fallback = offline_fallback_.Match(prompt);
        if (fallback.matched) {
          LOG(INFO) << "All LLM backends failed, "
                    << "using offline fallback";
          std::string fallback_result;

          if (!fallback.tool_name.empty()) {
            // Execute the matched tool directly
            auto it = tool_dispatch_.find(
                fallback.tool_name);
            if (it != tool_dispatch_.end()) {
              fallback_result = it->second(
                  fallback.args,
                  fallback.tool_name,
                  session_id);
            } else {
              fallback_result =
                  "{\"error\": \"Tool not found: " +
                  fallback.tool_name + "\"}";
            }
          }

          // Build response
          std::string reply;
          if (!fallback.direct_response.empty()) {
            reply = fallback.direct_response;
          } else {
            reply =
                "[Offline mode] " + fallback_result;
          }

          // Save to history
          {
            std::lock_guard<std::mutex> lock(
                session_mutex_);
            LlmMessage msg;
            msg.role = "assistant";
            msg.text = reply;
            sessions_[session_id].push_back(msg);
          }
          return reply;
        }

        // No fallback match — track error
        if (health_monitor_)
          health_monitor_->IncrementErrorCount();
        // Rollback: remove the user message
        {
          std::lock_guard<std::mutex> lock(session_mutex_);
          if (!sessions_[session_id].empty()) {
            sessions_[session_id].pop_back();
          }
        }
        return "Error: All language model backends "
               "are unavailable. " +
               resp.error_message;
      }
    }

    // Log token usage
    if (resp.total_tokens > 0) {
      session_store_.LogTokenUsage(session_id, current_backend->GetName(),
                                   resp.prompt_tokens, resp.completion_tokens);
      LOG(INFO) << "Tokens: prompt=" << resp.prompt_tokens
                << " completion=" << resp.completion_tokens
                << " total=" << resp.total_tokens;
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

        (void)session_store_.SaveSession(session_id, sessions_[session_id]);
      }

      LOG(INFO) << "Final response: " << resp.text;
      if (canvas_ipc_server_) {
        canvas_ipc_server_->BroadcastState("idle", "Response ready");
      }

      // Auto skill generation: detect capability gap
      // and attempt to create a new skill
      if (auto_skill_agent_ &&
          auto_skill_agent_->DetectCapabilityGap(resp.text)) {
        LOG(INFO) << "Capability gap detected, "
                  << "triggering auto skill generation";
        auto gen_result = auto_skill_agent_->TryGenerate(
            session_id, prompt, on_chunk);
        if (gen_result.success) {
          LOG(INFO) << "Auto skill generated: "
                    << gen_result.skill_name;
          return gen_result.output;
        }
        LOG(WARNING) << "Auto skill generation failed: "
                     << gen_result.error;
      }

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

    LOG(INFO) << "Iteration " << (iterations + 1) << ": Executing "
              << resp.tool_calls.size() << " tools in parallel";

    if (canvas_ipc_server_) {
      canvas_ipc_server_->BroadcastState(
          "tool_call",
          "Executing " + std::to_string(resp.tool_calls.size()) + " tools...");
    }

    // Execute multiple tools in parallel using std::async
    struct ToolExecResult {
      std::string id;
      std::string name;
      std::string output;
    };
    std::vector<std::future<ToolExecResult>> futures;
    for (auto& tc : resp.tool_calls) {
      futures.push_back(std::async(std::launch::async, [this, tc,
                                                        &session_id]() {
        ToolExecResult r;
        r.id = tc.id;
        r.name = tc.name;

        // Check tool execution policy
        std::string violation =
            tool_policy_.CheckPolicy(session_id, tc.name, tc.args);
        if (!violation.empty()) {
          LOG(WARNING) << "Tool blocked by policy: " << tc.name << " - "
                       << violation;
          r.output = "{\"error\": \"" + violation + "\"}";
          AuditLogger::Instance().Log(AuditLogger::MakeEvent(
              AuditEventType::kToolBlocked, session_id,
              {{"skill", tc.name}, {"reason", violation}}));
          return r;
        }

        // Look up user profile for RBAC
        std::string u_id = user_profile_store_.GetUserIdForSession(session_id);
        UserProfile profile = user_profile_store_.GetProfile(u_id);

        // Safety guard: physical bounds, device exclusion, RBAC
        auto safety_result =
            safety_guard_.Validate(tc.name, tc.args, profile.role);
        if (!safety_result.allowed) {
          LOG(WARNING) << "Tool blocked by SafetyGuard: "
                       << tc.name << " - "
                       << safety_result.reason;
          r.output = "{\"error\": \"" +
                     safety_result.reason + "\"}";
          AuditLogger::Instance().Log(
              AuditLogger::MakeEvent(
                  AuditEventType::kToolBlocked,
                  session_id,
                  {{"skill", tc.name},
                   {"reason", safety_result.reason},
                   {"layer", "safety_guard"}}));
          return r;
        }

        // Safety guard: action rate limiting
        if (safety_guard_.CheckActionRateLimit(
                tc.name)) {
          std::string rate_msg =
              "Action rate limit exceeded for " +
              tc.name +
              ". Please wait before retrying.";
          LOG(WARNING) << rate_msg;
          r.output = "{\"error\": \"" + rate_msg + "\"}";
          return r;
        }

        // Resolve tool name via ToolRouter
        std::string resolved_name =
            tool_router_.Resolve(tc.name);
        bool was_routed =
            (resolved_name != tc.name);
        std::string routed_hint;
        if (was_routed) {
          routed_hint =
              " [Routed: " + tc.name +
              " -> " + resolved_name + "]";
          r.name = resolved_name;
        }

        auto start = std::chrono::steady_clock::now();

        // Tool execution with retry (max 3 attempts)
        static constexpr int kMaxToolRetries = 1;
        static constexpr int kRetryBaseDelayMs = 200;
        for (int attempt = 0; attempt < kMaxToolRetries; ++attempt) {
          auto it = tool_dispatch_.find(resolved_name);
          if (it != tool_dispatch_.end()) {
            r.output = it->second(
                tc.args, resolved_name, session_id);
          } else if (resolved_name == "execute_action" ||
                     resolved_name.starts_with("action_")) {
            r.output = ExecuteActionOp(resolved_name, tc.args);
          } else if (McpClientManager::IsMcpTool(resolved_name)) {
            r.output = mcp_client_manager_->ExecuteTool(resolved_name, tc.args);
          } else {
            r.output = ExecuteSkill(resolved_name, tc.args);
          }

          // Check if execution failed
          bool has_error =
              r.output.find("\"error\"") != std::string::npos ||
              r.output.find("Skill failed") != std::string::npos ||
              r.output.empty();

          if (!has_error || attempt + 1 >= kMaxToolRetries) {
            if (has_error && attempt > 0) {
              LOG(WARNING) << "Tool '" << resolved_name
                           << "' failed after " << (attempt + 1)
                           << " attempts";
            }
            break;
          }

          // Exponential backoff before retry
          int delay_ms = kRetryBaseDelayMs * (1 << attempt);
          LOG(INFO) << "Tool '" << resolved_name
                    << "' failed (attempt " << (attempt + 1)
                    << "/" << kMaxToolRetries
                    << "), retrying in " << delay_ms << "ms";
          std::this_thread::sleep_for(
              std::chrono::milliseconds(delay_ms));
        }

        // Truncate large outputs to save LLM tokens
        static constexpr size_t kMaxToolOutputSize = 4096;
        if (r.output.size() > kMaxToolOutputSize) {
          LOG(INFO) << "Truncating tool output from "
                    << r.output.size() << " to "
                    << kMaxToolOutputSize << " bytes";
          r.output = r.output.substr(0, kMaxToolOutputSize) +
                     "\n... (truncated, " +
                     std::to_string(r.output.size()) +
                     " bytes total)";
        }

        // Append routing hint to output
        if (was_routed && !r.output.empty()) {
          r.output += routed_hint;
        }
        auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                           std::chrono::steady_clock::now() - start)
                           .count();
        // Log skill execution
        session_store_.LogSkillExecution(
            session_id, tc.name, tc.args,
            r.output.substr(0, std::min((size_t)200, r.output.size())),
            static_cast<int>(elapsed));
        // Record to episodic memory
        memory_store_.RecordSkillExecution(
            tc.name, tc.args,
            r.output.substr(
                0, std::min((size_t)200,
                            r.output.size())),
            r.output.find("\"error\"") ==
                std::string::npos,
            static_cast<int>(elapsed));
        AuditLogger::Instance().Log(AuditLogger::MakeEvent(
            AuditEventType::kToolExecution, session_id,
            {{"skill", tc.name},
             {"duration", std::to_string(elapsed) + "ms"}}));
        // Track tool call in health metrics
        if (health_monitor_) health_monitor_->IncrementToolCallCount();
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
      } catch (const nlohmann::json::exception&) {
        tool_msg.tool_result =
            {{"output", result.output}};
      }

      tool_msgs.push_back(tool_msg);
      local_history.push_back(tool_msg);
    }

    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      sessions_[session_id].insert(sessions_[session_id].end(),
                                   tool_msgs.begin(), tool_msgs.end());
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
    if (tool_policy_.CheckIdleProgress(session_id, iter_sig.str())) {
      LOG(WARNING) << "Idle loop detected in session: " << session_id;
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
        std::lock_guard<std::mutex> lock(session_mutex_);
        sessions_[session_id].push_back(stop_msg);
        (void)session_store_.SaveSession(session_id, sessions_[session_id]);
      }
      return idle_msg;
    }

    iterations++;
  }

  LOG(WARNING) << "Reached max tool iterations (" << max_iter << ")";

  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    (void)session_store_.SaveSession(session_id, sessions_[session_id]);
  }

  return last_text.empty() ? "Task partially completed "
                             "(reached iteration limit)."
                           : last_text;
}

std::string AgentCore::ExecuteSkill(const std::string& skill_name,
                                    const nlohmann::json& args) {
  LOG(INFO) << "Executing skill: " << skill_name;

  // Resolve manifest name to actual directory name
  std::string dir_name = skill_name;
  {
    std::lock_guard<std::mutex> lock(tools_mutex_);
    auto it = skill_dirs_.find(skill_name);
    if (it != skill_dirs_.end()) {
      dir_name = it->second;
    }
  }

  std::string arg_str = args.dump();
  std::string response = container_->ExecuteSkill(dir_name, arg_str);

  if (response.empty()) {
    LOG(ERROR) << "Skill execution failed";
    return "{\"error\": \"Skill failed\"}";
  }

  LOG(INFO) << "Skill output: " << response;
  return response;
}

std::string AgentCore::ExecuteCode(const std::string& code) {
  LOG(INFO) << "ExecuteCode: " << code.size() << " chars";

  std::string response = container_->ExecuteCode(code);

  if (response.empty()) {
    LOG(ERROR) << "Code execution failed";
    return "{\"error\": \"Code execution failed\"}";
  }

  LOG(INFO) << "Code output: " << response;
  return response;
}

void AgentCore::ReloadBackend() {
  LOG(INFO) << "Reloading active backend based on plugin changes...";
  if (!SwitchToBestBackend(true)) {
    LOG(ERROR) << "Failed to reload and switch to best backend.";
  }
}

struct BackendCandidate {
  std::string name;
  int priority;
  std::shared_ptr<PluginLlmBackend> plugin;
};

bool AgentCore::SwitchToBestBackend(bool is_reload) {
  std::string default_backend = llm_config_.value("active_backend", "openai");

  std::vector<BackendCandidate> candidates;
  // 1. Add active backend with baseline priority 1
  candidates.push_back({default_backend, 1, nullptr});

  // 2. Add fallback backends with configured priority (default 1)
  if (llm_config_.contains("fallback_backends")) {
    for (const auto& name_val : llm_config_["fallback_backends"]) {
      std::string fb = name_val.get<std::string>();
      if (fb == default_backend) continue;

      int prio = 1;
      if (llm_config_.contains("backends") &&
          llm_config_["backends"].contains(fb)) {
        prio = llm_config_["backends"][fb].value("priority", 1);
      }
      candidates.push_back({fb, prio, nullptr});
    }
  }

  // 3. Add plugin backends
  auto plugins = PluginManager::GetInstance().GetLlmBackends();
  for (auto& plugin : plugins) {
    if (!plugin) continue;
    int prio = plugin->GetConfig().value("priority", 0);
    candidates.push_back({plugin->GetName(), prio, plugin});
  }

  // 4. Sort descending by priority
  std::stable_sort(candidates.begin(), candidates.end(),
                   [](const BackendCandidate& a, const BackendCandidate& b) {
                     return a.priority > b.priority;
                   });

  // 5. Try to initialize the best backend
  std::lock_guard<std::mutex> lock(backend_mutex_);

  for (size_t i = 0; i < candidates.size(); ++i) {
    auto& cand = candidates[i];
    std::string bname = cand.name;

    std::shared_ptr<LlmBackend> new_backend = LlmBackendFactory::Create(bname);
    if (!new_backend) {
      LOG(WARNING) << "Failed to create backend: " << bname;
      continue;
    }

    nlohmann::json backend_config;
    if (cand.plugin) {
      backend_config = cand.plugin->GetConfig();
    } else if (llm_config_.contains("backends") &&
               llm_config_["backends"].contains(bname)) {
      backend_config = llm_config_["backends"][bname];
    }

    // Decrypt API key if encrypted
    if (backend_config.contains("api_key")) {
      std::string api_key = backend_config["api_key"].get<std::string>();
      if (KeyStore::IsEncrypted(api_key)) {
        std::string decrypted = KeyStore::Decrypt(api_key);
        if (!decrypted.empty()) {
          backend_config["api_key"] = decrypted;
          LOG(INFO) << "API key decrypted for: " << bname;
        } else {
          LOG(ERROR) << "Failed to decrypt API key for: " << bname;
        }
      }
    }

    // xAI injection
    if (bname == "xai" || bname == "grok") {
      backend_config["provider_name"] = "xai";
      if (!backend_config.contains("endpoint")) {
        backend_config["endpoint"] = "https://api.x.ai/v1";
      }
    }

    if (!new_backend->Initialize(backend_config)) {
      LOG(WARNING) << "Failed to initialize backend: " << bname;
      continue;
    }

    backend_ = std::move(new_backend);

    if (is_reload) {
      AuditLogger::Instance().Log(AuditLogger::MakeEvent(
          AuditEventType::kConfigChange, "",
          {{"backend", backend_->GetName()}}));
    }

    // Populate fallback_names_ with all other candidates
    // (including previously failed ones for runtime retry)
    fallback_names_.clear();
    for (size_t j = 0; j < candidates.size(); ++j) {
      if (j == i) continue;  // Skip the primary backend
      if (std::find(fallback_names_.begin(), fallback_names_.end(),
                    candidates[j].name) == fallback_names_.end()) {
        fallback_names_.push_back(candidates[j].name);
      }
    }
    if (!fallback_names_.empty()) {
      LOG(INFO) << "Fallback backends queues: " << fallback_names_.size();
    }

    return true;
  }

  LOG(ERROR) << "Failed to initialize ANY backend from candidates list!";
  return false;
}

std::string AgentCore::ExecuteFileOp(const std::string& operation,
                                     const std::string& path,
                                     const std::string& content) {
  LOG(INFO) << "ExecuteFileOp: op=" << operation << " path=" << path;

  std::string response = container_->ExecuteFileOp(operation, path, content);

  if (response.empty()) {
    LOG(ERROR) << "File operation failed";
    return "{\"error\": \"File operation failed\"}";
  }

  LOG(INFO) << "FileOp output: " << response;
  return response;
}

std::vector<LlmToolDecl> AgentCore::LoadSkillDeclarations() {
  // Return cached declarations after first load
  if (cached_tools_loaded_.load()) {
    std::lock_guard<std::mutex> lock(tools_mutex_);
    return cached_tools_;
  }

  std::lock_guard<std::mutex> lock(tools_mutex_);

  // Double-check after acquiring lock
  if (cached_tools_loaded_.load()) return cached_tools_;

  namespace fs = std::filesystem;

  std::vector<LlmToolDecl> tools;
  skill_runtimes_.clear();
  skill_dirs_.clear();
  const std::string skills_dir = "/opt/usr/share/tizen-tools/skills";

  // Lambda to scan a skills directory.
  // Supports both SKILL.md (Anthropic standard) and
  // manifest.json (legacy). SKILL.md takes priority.
  auto scan_dir = [&](const std::string& dir, CapabilitySource source_type) {
    std::error_code ec;
    if (!fs::is_directory(dir, ec)) return;

    for (const auto& entry : fs::directory_iterator(dir, ec)) {
      if (!entry.is_directory()) continue;
      auto dirname = entry.path().filename().string();
      if (dirname[0] == '.') continue;
      std::string skill_path = entry.path().string();

      // Load manifest (SKILL.md > manifest.json)
      nlohmann::json j = SkillManifest::Load(skill_path);
      if (j.empty()) continue;

      try {
        // Skip disabled skills
        if (j.contains("verified") &&
            j["verified"].is_boolean() &&
            !j["verified"].get<bool>()) {
          LOG(INFO) << "Skipping disabled skill: "
                    << dirname;
          continue;
        }

        if (j.contains("parameters")) {
          LlmToolDecl t;
          t.name = j.value("name", dirname);
          t.description = j.value("description", "");
          t.parameters = j["parameters"];
          tools.push_back(t);
          tool_policy_.LoadManifestRiskLevel(t.name, j);

          // Register to CapabilityRegistry
          Capability cap;
          cap.name = t.name;
          cap.description = t.description;
          cap.source = source_type;
          cap.category =
              j.contains("category")
                  ? j["category"].get<std::string>()
              : j.contains("metadata") &&
                j["metadata"].contains("category")
                  ? j["metadata"]["category"]
                        .get<std::string>()
                  : "general";
          if (j.contains("contract")) {
            cap.contract =
                CapabilityRegistry::ParseContract(
                    j["contract"]);
          }
          CapabilityRegistry::GetInstance()
              .Register(t.name, cap);

          // Track runtime for execution dispatch
          std::string runtime =
              j.value("runtime", "python");
          skill_runtimes_[t.name] = runtime;

          // Map manifest name -> directory name
          skill_dirs_[t.name] = dirname;
        }
      } catch (const std::exception& e) {
        LOG(WARNING) << "Failed to parse "
                     << "skill manifest: " << skill_path
                     << ": " << e.what();
      }
    }
  };

  scan_dir(skills_dir, CapabilitySource::kSkill);

  // Also scan custom_skills directory
  const std::string custom_skills_dir =
      "/opt/usr/share/tizen-tools/"
      "custom_skills";
  scan_dir(custom_skills_dir, CapabilitySource::kCustomSkill);

  // Append all built-in tool declarations
  ToolDeclarationBuilder::AppendBuiltinTools(tools);

  // Append Action Framework per-action tools
  ToolDeclarationBuilder::AppendActionTools(
      tools, action_bridge_.get());

  // Append CLI tools and scan tool.md docs
  ToolDeclarationBuilder::AppendCliTools(
      tools, cli_dirs_, cli_tool_docs_);

  if (mcp_client_manager_) {
    auto mcp_tools = mcp_client_manager_->GetToolDeclarations();
    if (!mcp_tools.empty()) {
      tools.insert(tools.end(), mcp_tools.begin(), mcp_tools.end());
      LOG(INFO) << "Loaded " << mcp_tools.size()
                << " remote MCP tool(s)";
    }
  }

  // Regenerate tool index files
  ToolIndexer::RegenerateAll(
      "/opt/usr/share/tizen-tools");

  // Detect and register capability overlaps
  // for automatic tool routing
  {
    auto overlaps =
        CapabilityRegistry::GetInstance()
            .DetectOverlaps();
    for (const auto& [lower, higher] : overlaps) {
      tool_router_.RegisterOverlap(lower, higher);
    }
    if (!overlaps.empty()) {
      LOG(INFO) << "ToolRouter: " << overlaps.size()
                << " auto-detected overlaps "
                << "registered";
    }
  }

  cached_tools_ = tools;
  cached_tools_loaded_.store(true);
  return tools;
}

void AgentCore::ReloadSkills() {
  LOG(INFO) << "Reloading skill declarations";
  {
    std::lock_guard<std::mutex> lock(tools_mutex_);
    cached_tools_.clear();
  }
  cached_tools_loaded_.store(false);

  // Force reload, regenerate indexes, and rebuild
  // system prompt
  auto tools = LoadSkillDeclarations();
  system_prompt_ = BuildSystemPrompt(tools);
  LOG(INFO) << "Skill reload complete: "
            << tools.size() << " tools";
}

std::string AgentCore::LoadSystemPrompt(const nlohmann::json& config) {
  // Priority 1: Inline system_prompt in config
  if (config.contains("system_prompt") && config["system_prompt"].is_string()) {
    std::string prompt = config["system_prompt"].get<std::string>();
    if (!prompt.empty()) {
      LOG(INFO) << "System prompt loaded from config (inline)";
      return prompt;
    }
  }

  // Priority 2: system_prompt_file path in config
  if (config.contains("system_prompt_file") &&
      config["system_prompt_file"].is_string()) {
    std::string file_path = config["system_prompt_file"].get<std::string>();
    std::ifstream pf(file_path);
    if (pf.is_open()) {
      std::string content((std::istreambuf_iterator<char>(pf)),
                          std::istreambuf_iterator<char>());
      pf.close();
      if (!content.empty()) {
        LOG(INFO) << "System prompt loaded from: " << file_path;
        return content;
      }
    }
  }

  // Priority 3: Hardcoded fallback
  LOG(INFO) << "Using hardcoded default system prompt";
  return "You are TizenClaw, an AI assistant running "
         "on a Tizen device. You can control the device "
         "using the available tools. You possess extensive "
         "documentation on Tizen APIs in your knowledge base; "
         "use the search_knowledge tool for Tizen Native API queries. "
         "Always respond in the same language as the user's message. "
         "Be concise and helpful.";
}

std::string AgentCore::LoadRoutingGuide() {
  const std::string guide_path =
      "/opt/usr/share/tizen-tools/routing_guide.md";
  std::ifstream f(guide_path);
  if (f.is_open()) {
    std::string content((std::istreambuf_iterator<char>(f)),
                        std::istreambuf_iterator<char>());
    f.close();
    if (!content.empty()) {
      return "\n\n## Tool Selection Strategy\n" + content;
    }
  }
  return "";
}

std::string AgentCore::LoadWebApiCatalog() {
  const std::string index_path =
      std::string(APP_DATA_DIR) + "/rag/web/index.md";
  std::ifstream f(index_path);
  if (!f.is_open()) return "";

  // Parse index.md: extract ## and ### headings
  // and collect API names under each category
  std::string catalog;
  catalog += "\n\n## Tizen Web API Reference\n";
  catalog += "You have access to Tizen Web API docs. ";
  catalog += "Use `lookup_web_api` tool to read them.\n";
  catalog += "- operation=\"read\", path=\"<path>\" ";
  catalog += "to read a specific doc\n";
  catalog += "- operation=\"search\", query=\"<keyword>\" ";
  catalog += "to search by keyword\n\n";

  std::string line;
  std::string current_h2;
  std::string current_h3;
  std::vector<std::string> items;

  auto flush_section = [&]() {
    if (!current_h3.empty() && !items.empty()) {
      catalog += "- **" + current_h3 + "**: ";
      // Show first few items, then count
      const size_t MAX_SHOW = 5;
      for (size_t i = 0;
           i < std::min(MAX_SHOW, items.size()); i++) {
        if (i > 0) catalog += ", ";
        catalog += items[i];
      }
      if (items.size() > MAX_SHOW) {
        catalog += " (+ " +
            std::to_string(items.size() - MAX_SHOW) +
            " more)";
      }
      catalog += "\n";
    }
    items.clear();
  };

  while (std::getline(f, line)) {
    if (line.substr(0, 3) == "## " &&
        line.substr(0, 4) != "### ") {
      flush_section();
      current_h2 = line.substr(3);
      catalog += "\n### " + current_h2 + "\n";
      current_h3.clear();
    } else if (line.substr(0, 4) == "### ") {
      flush_section();
      current_h3 = line.substr(4);
    } else if (line.size() > 4 &&
               line.substr(0, 3) == "- [") {
      // Extract: - [Name](path) -> name and path
      auto bracket_end = line.find(']');
      auto paren_start = line.find('(');
      auto paren_end = line.find(')');
      if (bracket_end != std::string::npos &&
          paren_start != std::string::npos &&
          paren_end != std::string::npos) {
        std::string name =
            line.substr(3, bracket_end - 3);
        std::string path = line.substr(
            paren_start + 1,
            paren_end - paren_start - 1);
        // For first item in a section, show path
        if (items.empty()) {
          items.push_back(
              name + " (`" + path + "`)" );
        } else {
          items.push_back(name);
        }
      }
    }
  }
  flush_section();
  f.close();

  catalog += "\n> IMPORTANT: When generating Tizen web ";
  catalog += "app code, ALWAYS call lookup_web_api ";
  catalog += "with operation=\"read\" to read the ";
  catalog += "relevant API document BEFORE writing ";
  catalog += "code. The paths shown above are the ";
  catalog += "exact paths to use.\n";

  return catalog;
}

std::string AgentCore::BuildSystemPrompt(
    const std::vector<LlmToolDecl>& tools) {
  std::string prompt = system_prompt_ +
      LoadRoutingGuide() + web_api_catalog_;

  // Build tool list string
  std::string tool_list;
  for (const auto& t : tools) {
    tool_list += "- " + t.name + ": " + t.description + "\n";
  }

  // Load aggregated tool catalog from tools.md
  {
    const std::string tools_md_path =
        "/opt/usr/share/tizen-tools/tools.md";
    std::ifstream in(tools_md_path);
    if (in.is_open()) {
      std::string catalog(
          (std::istreambuf_iterator<char>(in)),
          std::istreambuf_iterator<char>());
      if (!catalog.empty()) {
        tool_list += "\n" + catalog + "\n";
      }
    }
  }

  // Inject CLI tool documentation into prompt
  {
    std::lock_guard<std::mutex> lock(tools_mutex_);
    if (!cli_tool_docs_.empty()) {
      tool_list += "\n## CLI Tools\n";
      tool_list += "Use the `execute_cli` tool to ";
      tool_list += "invoke these CLI tools.\n\n";
      for (const auto& [name, doc] : cli_tool_docs_) {
        tool_list += doc + "\n\n---\n\n";
      }
    }
  }

  // Inject system CLI tool documentation into prompt
  {
    auto sys_docs = SystemCliAdapter::GetInstance().GetToolDocs();
    if (!sys_docs.empty()) {
      tool_list += "\n## System CLI Tools\n";
      tool_list += "Use the `execute_cli` tool to ";
      tool_list += "invoke these system tools installed on the device.\n\n";
      for (const auto& [name, doc] : sys_docs) {
        tool_list += doc + "\n\n---\n\n";
      }
    }
  }

  // Replace {{MEMORY_CONTEXT}} placeholder
  const std::string mem_ph = "{{MEMORY_CONTEXT}}";
  size_t mem_pos = prompt.find(mem_ph);
  if (mem_pos != std::string::npos) {
    prompt.replace(mem_pos, mem_ph.size(),
                   memory_store_.LoadSummary());
  }

  // Replace {{SYSTEM_CONTEXT}} placeholder
  {
    const std::string sys_ph = "{{SYSTEM_CONTEXT}}";
    size_t sys_pos = prompt.find(sys_ph);
    std::string sys_ctx;
    if (system_context_) {
      sys_ctx = system_context_->GetContextString();
    }
    if (sys_pos != std::string::npos) {
      prompt.replace(sys_pos, sys_ph.size(), sys_ctx);
    } else if (!sys_ctx.empty()) {
      // If no placeholder, append system context
      prompt += "\n\n## Current System Context\n" + sys_ctx;
    }
  }

  // Replace {{CAPABILITY_SUMMARY}} placeholder
  {
    const std::string cap_ph =
        "{{CAPABILITY_SUMMARY}}";
    size_t cap_pos = prompt.find(cap_ph);
    std::string cap_ctx;
    auto summary =
        CapabilityRegistry::GetInstance()
            .GetCapabilitySummary();
    if (!summary.empty())
      cap_ctx = summary.dump(2);
    if (cap_pos != std::string::npos) {
      prompt.replace(
          cap_pos, cap_ph.size(), cap_ctx);
    } else if (!cap_ctx.empty()) {
      prompt +=
          "\n\n## Tool Capabilities\n" +
          cap_ctx;
    }
  }

  // Replace {{AVAILABLE_TOOLS}} placeholder
  const std::string placeholder = "{{AVAILABLE_TOOLS}}";
  size_t pos = prompt.find(placeholder);
  if (pos != std::string::npos) {
    prompt.replace(pos, placeholder.size(), tool_list);
  } else if (!tool_list.empty()) {
    // If no placeholder, append tool list
    prompt += "\n\nAvailable tools:\n" + tool_list;
  }

  return prompt;
}

void AgentCore::CompactHistory(const std::string& session_id) {
  // MUST be called with session_mutex_ held
  auto& history = sessions_[session_id];
  if (history.size() <= kCompactionThreshold) {
    return;  // Below threshold, no compaction
  }
  if (!backend_) return;

  // Prevent concurrent compaction/trim for the same session
  if (session_compacting_.contains(session_id)) {
    return;
  }
  session_compacting_.insert(session_id);

  // Gather oldest N messages to summarize
  size_t count = std::min(kCompactionCount, history.size() - 2);
  if (count < 2) return;

  // Extend count to include complete
  // tool_call/tool pairs — never split a pair
  while (count < history.size() - 2) {
    auto& last = history[count - 1];
    if (last.role == "assistant" && !last.tool_calls.empty()) {
      // Include all following tool results
      while (count < history.size() - 2 && history[count].role == "tool") {
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

  std::vector<LlmMessage> to_summarize(history.begin(),
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
    if (msg.role == "tool" && !msg.tool_name.empty()) {
      summary_prompt += "[" + msg.tool_name + " result]";
    }
    summary_prompt += "\n";
  }

  LlmMessage compact_prompt;
  compact_prompt.role = "user";
  compact_prompt.text =
      "Summarize this conversation concisely "
      "in 2-3 sentences. Preserve key facts, "
      "decisions, and results. Respond ONLY "
      "with the summary, nothing else:\n\n" +
      summary_prompt;

  std::vector<LlmMessage> compact_msgs;
  compact_msgs.push_back(compact_prompt);

  // Release lock temporarily for LLM call
  // (we hold a copy of what we need)
  session_mutex_.unlock();

  LlmResponse resp;
  try {
    resp = backend_->Chat(compact_msgs, {},  // no tools for compaction
                          nullptr,           // no streaming
                          "");               // no system prompt
  } catch (const std::exception& e) {
    session_mutex_.lock();
    LOG(WARNING) << "Compaction LLM call failed: "
                 << e.what();
    return;
  }

  session_mutex_.lock();

  // Ensure we clear the compacting flag
  session_compacting_.erase(session_id);

  // Verify session still exists after re-lock
  if (!sessions_.contains(session_id)) {
    return;
  }
  auto& hist = sessions_[session_id];

  if (!resp.success || resp.text.empty()) {
    LOG(WARNING) << "Compaction failed, " << "falling back to FIFO";
    // Fallback: simple FIFO trim
    while (hist.size() > kCompactionThreshold && !hist.empty()) {
      hist.erase(hist.begin());
    }
    return;
  }

  // Safety check to prevent out-of-bounds erase
  if (hist.size() < count) {
    return;
  }

  // Replace oldest N turns with 1 compressed
  LlmMessage compressed;
  compressed.role = "assistant";
  compressed.text = "[compressed] " + resp.text;

  hist.erase(hist.begin(), hist.begin() + count);
  hist.insert(hist.begin(), compressed);

  LOG(INFO) << "Compacted " << count
            << " turns into 1 for session: " << session_id;

  // Log compaction token usage if available
  if (resp.total_tokens > 0) {
    session_store_.LogTokenUsage(session_id,
                                 backend_->GetName() + "_compaction",
                                 resp.prompt_tokens, resp.completion_tokens);
  }
}

void AgentCore::TrimHistory(const std::string& session_id) {
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

void AgentCore::ClearSession(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(session_mutex_);
  sessions_.erase(session_id);
  session_store_.DeleteSession(session_id);
  tool_policy_.ResetSession(session_id);
}

std::string AgentCore::ExecuteSkillForMcp(const std::string& skill_name,
                                          const nlohmann::json& args) {
  UpdateActivityTime();
  return ExecuteSkill(skill_name, args);
}

std::string AgentCore::ExecuteTaskOp(const std::string& operation,
                                     const nlohmann::json& args) {
  if (!scheduler_) {
    return "{\"error\": "
           "\"TaskScheduler not available\"}";
  }

  nlohmann::json result;

  if (operation == "create_task") {
    std::string schedule = args.value("schedule", "");
    std::string prompt = args.value("prompt", "");
    std::string session_id = args.value("session_id", "default");

    if (schedule.empty() || prompt.empty()) {
      result = {{"error", "schedule and prompt are required"}};
    } else {
      std::string task_id =
          scheduler_->CreateTask(schedule, prompt, session_id);
      if (task_id.empty()) {
        result = {{"error",
                   "Invalid schedule expression. "
                   "Use: 'daily HH:MM', "
                   "'interval Ns/Nm/Nh', "
                   "'once YYYY-MM-DD HH:MM', or "
                   "'weekly DAY HH:MM'"}};
      } else {
        result = {{"status", "created"},
                  {"task_id", task_id},
                  {"schedule", schedule},
                  {"prompt", prompt}};
      }
    }
  } else if (operation == "list_tasks") {
    std::string session_id = args.value("session_id", "");
    auto tasks = scheduler_->ListTasks(session_id);
    result = {{"status", "ok"}, {"tasks", tasks}, {"count", tasks.size()}};
  } else if (operation == "cancel_task") {
    std::string task_id = args.value("task_id", "");
    if (task_id.empty()) {
      result = {{"error", "task_id is required"}};
    } else {
      bool ok = scheduler_->CancelTask(task_id);
      if (ok) {
        result = {{"status", "cancelled"}, {"task_id", task_id}};
      } else {
        result = {{"error", "Task not found"}, {"task_id", task_id}};
      }
    }
  } else {
    result = {{"error", "Unknown task operation: " + operation}};
  }

  return result.dump();
}

LlmResponse AgentCore::TryFallbackBackends(
    const std::vector<LlmMessage>& history,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  LlmResponse last_resp;
  last_resp.success = false;
  last_resp.error_message = "No fallback backends configured";

  if (fallback_names_.empty()) {
    LOG(WARNING) << "No fallback backends available; "
                << "primary backend failure is unrecoverable";
    return last_resp;
  }

  for (const auto& fb_name : fallback_names_) {
    LOG(INFO) << "Trying fallback backend: " << fb_name;

    auto fb_backend = LlmBackendFactory::Create(fb_name);
    if (!fb_backend) {
      LOG(WARNING) << "Failed to create fallback: " << fb_name;
      continue;
    }

    // Get backend config
    nlohmann::json fb_config;
    if (llm_config_.contains("backends") &&
        llm_config_["backends"].contains(fb_name)) {
      fb_config = llm_config_["backends"][fb_name];
    }

    // Decrypt API key if encrypted
    if (fb_config.contains("api_key")) {
      std::string api_key = fb_config["api_key"].get<std::string>();
      if (KeyStore::IsEncrypted(api_key)) {
        std::string decrypted = KeyStore::Decrypt(api_key);
        if (!decrypted.empty()) {
          fb_config["api_key"] = decrypted;
        }
      }
    }

    // xAI identity injection
    if (fb_name == "xai" || fb_name == "grok") {
      fb_config["provider_name"] = "xai";
      if (!fb_config.contains("endpoint")) {
        fb_config["endpoint"] = "https://api.x.ai/v1";
      }
    }

    if (!fb_backend->Initialize(fb_config)) {
      LOG(WARNING) << "Failed to init fallback: " << fb_name;
      continue;
    }

    // Rate-limit backoff: try with delay
    int backoff_ms = 0;
    if (last_resp.http_status == 429) {
      backoff_ms = 1000;  // 1s initial backoff
      LOG(INFO) << "Rate-limited, backing off " << backoff_ms << "ms";
      std::this_thread::sleep_for(std::chrono::milliseconds(backoff_ms));
    }

    LlmResponse resp =
        fb_backend->Chat(history, tools, on_chunk, system_prompt);

    if (resp.success) {
      LOG(INFO) << "Fallback succeeded: " << fb_name;

      // Switch primary backend
      backend_ = std::move(fb_backend);

      AuditLogger::Instance().Log(AuditLogger::MakeEvent(
          AuditEventType::kConfigChange, "",
          {{"fallback_from", "primary"}, {"fallback_to", fb_name}}));

      return resp;
    }

    last_resp = resp;
    LOG(WARNING) << "Fallback failed (" << fb_name
                 << "): " << resp.error_message;
  }

  return last_resp;
}

std::string AgentCore::ExecuteCliSessionOp(
    const std::string& operation, const nlohmann::json& args) {
  if (operation == "start") {
    std::string tool = args.value("tool_name", "");
    std::string arguments = args.value("arguments", "");
    std::string mode = args.value("mode", "interactive");
    int timeout = args.value("timeout", 60);
    return container_->StartCliSession(tool, arguments, mode, timeout);
  } else if (operation == "send") {
    std::string sid = args.value("session_id", "");
    std::string input = args.value("input", "");
    int read_timeout = args.value("read_timeout_ms", 2000);
    return container_->SendToCliSession(sid, input, read_timeout);
  } else if (operation == "read") {
    std::string sid = args.value("session_id", "");
    int read_timeout = args.value("read_timeout_ms", 1000);
    return container_->ReadCliSession(sid, read_timeout);
  } else if (operation == "close") {
    std::string sid = args.value("session_id", "");
    return container_->CloseCliSession(sid);
  }
  return "{\"status\":\"error\",\"output\":\"Unknown session operation\"}";
}

std::string AgentCore::ExecuteSessionOp(const std::string& operation,
                                        const nlohmann::json& args,
                                        const std::string& caller_session) {
  LOG(INFO) << "SessionOp: " << operation;

  if (operation == "create_session") {
    std::string name = args.value("name", "agent");
    std::string prompt = args.value("system_prompt", "");

    if (prompt.empty()) {
      return "{\"error\": \"system_prompt "
             "is required\"}";
    }

    // Generate unique session_id
    auto now = std::chrono::system_clock::now();
    auto ts = std::chrono::duration_cast<std::chrono::milliseconds>(
                  now.time_since_epoch())
                  .count();
    std::string session_id =
        "agent_" + name + "_" + std::to_string(ts % 100000);

    // Store per-session system prompt
    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      session_prompts_[session_id] = prompt;
    }

    LOG(INFO) << "Created agent session: " << session_id;

    nlohmann::json result = {{"status", "ok"},
                             {"session_id", session_id},
                             {"name", name},
                             {"system_prompt_length", (int)prompt.size()}};
    return result.dump();
  }

  if (operation == "list_sessions") {
    nlohmann::json sessions = nlohmann::json::array();
    {
      std::lock_guard<std::mutex> lock(session_mutex_);
      for (auto& [sid, prompt] : session_prompts_) {
        nlohmann::json s = {
            {"session_id", sid},
            {"system_prompt",
             prompt.substr(0, std::min((size_t)100, prompt.size())) +
                 (prompt.size() > 100 ? "..." : "")},
            {"history_size",
             sessions_.contains(sid) ? (int)sessions_[sid].size() : 0}};
        sessions.push_back(s);
      }

      // Also list sessions without custom
      // prompts (default sessions)
      for (auto& [sid, hist] : sessions_) {
        if (!session_prompts_.contains(sid)) {
          nlohmann::json s = {{"session_id", sid},
                              {"system_prompt", "(default)"},
                              {"history_size", (int)hist.size()}};
          sessions.push_back(s);
        }
      }
    }

    nlohmann::json result = {{"status", "ok"},
                             {"sessions", sessions},
                             {"count", (int)sessions.size()}};
    return result.dump();
  }

  if (operation == "send_to_session") {
    std::string target = args.value("target_session", "");
    std::string message = args.value("message", "");

    if (target.empty() || message.empty()) {
      return "{\"error\": \"target_session "
             "and message are required\"}";
    }

    // Prevent self-messaging loop
    if (target == caller_session) {
      return "{\"error\": \"Cannot send "
             "message to self\"}";
    }

    LOG(INFO) << "Sending to session: " << target
              << " from: " << caller_session;

    // Call ProcessPrompt on target session
    // Note: no streaming for inter-agent
    std::string response = ProcessPrompt(target, message);

    nlohmann::json result = {
        {"status", "ok"}, {"target_session", target}, {"response", response}};
    return result.dump();
  }

  return "{\"error\": \"Unknown session "
         "operation: " +
         operation + "\"}";
}

std::string AgentCore::GetSessionPrompt(const std::string& session_id,
                                        const std::vector<LlmToolDecl>& tools) {
  std::string base_prompt;
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    auto it = session_prompts_.find(session_id);
    if (it != session_prompts_.end()) {
      base_prompt = it->second;
      std::string tool_list;
      for (const auto& t : tools) {
        tool_list += "- " + t.name + ": " + t.description + "\n";
      }

      const std::string placeholder = "{{AVAILABLE_TOOLS}}";
      size_t pos = base_prompt.find(placeholder);
      if (pos != std::string::npos) {
        base_prompt.replace(pos, placeholder.size(), tool_list);
      } else if (!tool_list.empty()) {
        base_prompt += "\n\nAvailable tools:\n" + tool_list;
      }
    }
  }

  if (base_prompt.empty()) {
    base_prompt = BuildSystemPrompt(tools);
  }

  // Inject user profile context
  std::string u_id = user_profile_store_.GetUserIdForSession(session_id);
  UserProfile profile = user_profile_store_.GetProfile(u_id);

  std::string user_context = "\n\n[USER CONTEXT]\n";
  user_context += "Current User ID: " + profile.user_id + "\n";
  user_context += "User Name: " + profile.name + "\n";
  user_context += "User Role: " + UserProfileStore::RoleToString(profile.role) + "\n";
  if (!profile.preferences.empty()) {
    user_context += "Preferences: " + profile.preferences.dump() + "\n";
  }
  user_context += "Note: Keep the role and preferences in mind when performing actions.\n";

  return base_prompt + user_context;
}

std::string AgentCore::ExecuteRagOp(const std::string& operation,
                                    const nlohmann::json& args) {
  if (operation == "ingest_document") {
    std::string source = args.value("source", "");
    std::string text = args.value("text", "");

    if (source.empty() || text.empty()) {
      return "{\"error\": \"source and text "
             "are required\"}";
    }

    auto chunks = EmbeddingStore::ChunkText(text);
    int stored = 0;
    for (const auto& chunk : chunks) {
      auto emb = GenerateEmbedding(chunk);
      if (emb.empty()) {
        LOG(WARNING) << "Failed to generate embedding " << "for chunk ("
                     << chunk.size() << " chars)";
        continue;
      }
      if (embedding_store_.StoreChunk(source, chunk, emb)) {
        stored++;
      }
    }

    nlohmann::json result = {
        {"status", "ok"},
        {"source", source},
        {"chunks_total", static_cast<int>(chunks.size())},
        {"chunks_stored", stored},
        {"total_documents", embedding_store_.GetChunkCount()}};
    return result.dump();
  }

  if (operation == "search_knowledge") {
    std::string query = args.value("query", "");
    int top_k = args.value("top_k", 5);

    if (query.empty()) {
      return "{\"error\": \"query is required\"}";
    }

    auto query_emb = GenerateEmbedding(query);
    if (query_emb.empty()) {
      return "{\"error\": \"Failed to generate "
             "query embedding\"}";
    }

    auto results = embedding_store_.Search(query_emb, top_k);

    nlohmann::json j_results = nlohmann::json::array();
    for (const auto& r : results) {
      j_results.push_back(
          {{"source", r.source}, {"text", r.chunk_text}, {"score", r.score}});
    }

    return nlohmann::json(
               {{"status", "ok"}, {"query", query}, {"results", j_results}})
        .dump();
  }

  return "{\"error\": \"Unknown RAG operation\"}";
}

std::string AgentCore::ExecuteWebApiLookup(
    const std::string& operation,
    const nlohmann::json& args) {
  const std::string rag_base =
      std::string(APP_DATA_DIR) + "/rag/web";

  if (operation == "list") {
    // Return index.md contents
    std::string index_path = rag_base + "/index.md";
    std::ifstream f(index_path);
    if (!f.is_open()) {
      return "{\"error\": \"Web API index not found. "
             "RAG data may not be installed.\"}";
    }
    std::string content(
        (std::istreambuf_iterator<char>(f)),
        std::istreambuf_iterator<char>());
    f.close();
    return nlohmann::json(
               {{"status", "ok"},
                {"operation", "list"},
                {"content", content}})
        .dump();
  }

  if (operation == "read") {
    std::string rel_path = args.value("path", "");
    if (rel_path.empty()) {
      return "{\"error\": \"path is required "
             "for read operation\"}";
    }

    // Path traversal protection
    if (rel_path.find("..") != std::string::npos) {
      return "{\"error\": \"Invalid path\"}";
    }

    std::string full_path = rag_base + "/" + rel_path;
    std::ifstream f(full_path);
    if (!f.is_open()) {
      return nlohmann::json(
                 {{"error",
                   "File not found: " + rel_path}})
          .dump();
    }
    std::string content(
        (std::istreambuf_iterator<char>(f)),
        std::istreambuf_iterator<char>());
    f.close();

    // Truncate very large files to prevent
    // context overflow
    const size_t MAX_CONTENT = 32000;
    bool truncated = false;
    if (content.size() > MAX_CONTENT) {
      content = content.substr(0, MAX_CONTENT);
      truncated = true;
    }

    return nlohmann::json(
               {{"status", "ok"},
                {"operation", "read"},
                {"path", rel_path},
                {"content", content},
                {"truncated", truncated}})
        .dump();
  }

  if (operation == "search") {
    std::string query = args.value("query", "");
    if (query.empty()) {
      return "{\"error\": \"query is required "
             "for search operation\"}";
    }

    // Case-insensitive keyword search
    std::string query_lower = query;
    std::transform(query_lower.begin(),
                   query_lower.end(),
                   query_lower.begin(), ::tolower);

    nlohmann::json matches = nlohmann::json::array();
    namespace fs = std::filesystem;
    std::error_code ec;
    int max_results = 20;

    for (const auto& entry :
         fs::recursive_directory_iterator(
             rag_base, ec)) {
      if (!entry.is_regular_file()) continue;
      if (entry.path().extension() != ".md") continue;
      if (matches.size() >= static_cast<size_t>(
              max_results))
        break;

      std::ifstream f(entry.path());
      if (!f.is_open()) continue;

      std::string line;
      int line_num = 0;
      bool found = false;
      std::string snippet;

      while (std::getline(f, line)) {
        line_num++;
        std::string line_lower = line;
        std::transform(line_lower.begin(),
                       line_lower.end(),
                       line_lower.begin(),
                       ::tolower);
        if (line_lower.find(query_lower) !=
            std::string::npos) {
          if (!found) {
            snippet = line.substr(
                0, std::min((size_t)200,
                            line.size()));
            found = true;
          }
        }
      }
      f.close();

      if (found) {
        std::string rel = fs::relative(
            entry.path(), rag_base, ec).string();
        matches.push_back(
            {{"path", rel}, {"snippet", snippet}});
      }
    }

    return nlohmann::json(
               {{"status", "ok"},
                {"operation", "search"},
                {"query", query},
                {"results", matches},
                {"total", matches.size()}})
        .dump();
  }

  return "{\"error\": \"Unknown operation. "
         "Use: list, read, or search\"}";
}

std::vector<float> AgentCore::GenerateEmbedding(const std::string& text) {
  // Prefer on-device embedding (LLM-independent)
  {
    std::lock_guard<std::mutex> lock(embedding_mutex_);
    if (!on_device_embedding_.IsAvailable()) {
      std::string model_dir = std::string(APP_DATA_DIR) + "/models/all-MiniLM-L6-v2";
      std::string ort_lib = std::string(APP_DATA_DIR) + "/lib/libonnxruntime.so";
      if (!on_device_embedding_.Initialize(model_dir, ort_lib)) {
        LOG(WARNING) << "On-device embedding lazy-init failed";
      } else {
        LOG(INFO) << "On-device embedding lazily initialized";
      }
    }
  }

  if (on_device_embedding_.IsAvailable()) {
    std::lock_guard<std::mutex> lock(embedding_mutex_);
    return on_device_embedding_.Encode(text);
  }

  if (!backend_ || text.empty()) return {};

  std::string backend_name = backend_->GetName();

  // Determine embedding API endpoint + model
  std::string url;
  std::string model;
  std::string api_key;

  // Get backend config
  nlohmann::json bc;
  if (llm_config_.contains("backends") &&
      llm_config_["backends"].contains(backend_name)) {
    bc = llm_config_["backends"][backend_name];
  }
  api_key = bc.value("api_key", "");
  if (KeyStore::IsEncrypted(api_key)) {
    api_key = KeyStore::Decrypt(api_key);
  }

  nlohmann::json req_body;
  std::map<std::string, std::string> headers;

  if (backend_name == "gemini") {
    model = bc.value("embedding_model", "text-embedding-004");
    url =
        "https://generativelanguage.googleapis"
        ".com/v1beta/models/" +
        model + ":embedContent?key=" + api_key;
    req_body = {{"model", "models/" + model},
                {"content", {{"parts", {{{"text", text}}}}}}};
  } else if (backend_name == "openai" || backend_name == "xai" ||
             backend_name == "grok") {
    std::string endpoint = bc.value("endpoint", "https://api.openai.com/v1");
    model = bc.value("embedding_model", "text-embedding-3-small");
    url = endpoint + "/embeddings";
    req_body = {{"model", model}, {"input", text}};
    headers = {{"Authorization", "Bearer " + api_key},
               {"Content-Type", "application/json"}};
  } else if (backend_name == "ollama") {
    std::string endpoint = bc.value("endpoint", "http://localhost:11434");
    model = bc.value("embedding_model", "nomic-embed-text");
    url = endpoint + "/api/embeddings";
    req_body = {{"model", model}, {"prompt", text}};
    headers = {{"Content-Type", "application/json"}};
  } else {
    LOG(WARNING) << "No embedding support for: " << backend_name;
    return {};
  }

  if (backend_name == "gemini") {
    headers = {{"Content-Type", "application/json"}};
  }

  auto resp = HttpClient::Post(url, headers, req_body.dump(), 2);

  if (!resp.success) {
    LOG(ERROR) << "Embedding API failed: " << resp.error;
    return {};
  }

  try {
    auto j = nlohmann::json::parse(resp.body);
    std::vector<float> emb;

    if (backend_name == "gemini") {
      // Response: {"embedding":{"values":[...]}}
      if (j.contains("embedding") && j["embedding"].contains("values")) {
        for (auto& v : j["embedding"]["values"]) {
          emb.push_back(v.get<float>());
        }
      }
    } else if (backend_name == "openai" || backend_name == "xai" ||
               backend_name == "grok") {
      // Response: {"data":[{"embedding":[...]}]}
      if (j.contains("data") && !j["data"].empty() &&
          j["data"][0].contains("embedding")) {
        for (auto& v : j["data"][0]["embedding"]) {
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
      LOG(WARNING) << "Empty embedding from: " << backend_name;
    }
    return emb;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Embedding parse error: " << e.what();
    return {};
  }
}

std::string AgentCore::ExecuteSupervisorOp(const std::string& operation,
                                           const nlohmann::json& args,
                                           const std::string& session_id) {
  if (!supervisor_) {
    return "{\"error\": "
           "\"SupervisorEngine not available\"}";
  }

  nlohmann::json result;

  if (operation == "run_supervisor") {
    std::string goal = args.value("goal", "");
    std::string strategy = args.value("strategy", "sequential");

    if (goal.empty()) {
      result = {{"error", "goal is required"}};
    } else {
      std::string response =
          supervisor_->RunSupervisor(goal, strategy, session_id);
      result = {{"status", "ok"},
                {"goal", goal},
                {"strategy", strategy},
                {"result", response}};
    }
  } else if (operation == "list_agent_roles") {
    auto roles = supervisor_->ListRoles();
    result = {{"status", "ok"}, {"roles", roles}, {"count", (int)roles.size()}};
  } else if (operation == "get_agent_status") {
    auto status = supervisor_->GetAgentStatus();
    auto delegations =
        supervisor_->ListActiveDelegations();
    result = {{"status", "ok"},
              {"agent_status", status},
              {"delegations", delegations}};
  } else if (operation == "list_agents") {
    // Configured roles
    auto roles = supervisor_->ListRoles();

    // Dynamic agents
    nlohmann::json dynamic_agents =
        nlohmann::json::array();
    if (agent_factory_)
      dynamic_agents =
          agent_factory_->ListDynamicAgents();

    // Active delegations
    auto delegations =
        supervisor_->ListActiveDelegations();

    // Swarm peers
    nlohmann::json swarm_peers = nlohmann::json::array();
    if (swarm_manager_) {
      swarm_peers = swarm_manager_->GetStatusJson()["active_peers"];
    }

    // Event bus sources
    auto sources =
        EventBus::GetInstance().ListEventSources();
    auto bus_sources = nlohmann::json::array();
    for (const auto& s : sources) {
      bus_sources.push_back(
          {{"name", s.name},
           {"plugin_id", s.plugin_id},
           {"collect_method", s.collect_method}});
    }

    // Autonomous trigger status
    nlohmann::json trigger_info =
        {{"enabled", false}};
    if (system_context_) {
      trigger_info["system_context"] = true;
    }

    result = {
        {"status", "ok"},
        {"configured_roles", roles},
        {"configured_roles_count",
         (int)roles.size()},
        {"dynamic_agents", dynamic_agents},
        {"dynamic_agents_count",
         (int)dynamic_agents.size()},
        {"active_delegations", delegations},
        {"event_bus_sources", bus_sources},
        {"event_bus_sources_count",
         (int)bus_sources.size()},
        {"autonomous_trigger", trigger_info}};
  } else {
    result = {{"error", "Unknown supervisor operation: " + operation}};
  }

  return result.dump();
}

std::vector<LlmToolDecl> AgentCore::GetToolsFiltered(
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

std::string AgentCore::ExecutePipelineOp(const std::string& operation,
                                         const nlohmann::json& args,
                                         const std::string& session_id) {
  (void)session_id;  // Reserved for future use
  if (!pipeline_executor_) {
    return "{\"error\": "
           "\"PipelineExecutor not available\"}";
  }

  nlohmann::json result;

  if (operation == "create_pipeline") {
    std::string id = pipeline_executor_->CreatePipeline(args);
    if (id.empty()) {
      result = {{"error",
                 "Failed to create pipeline. "
                 "name and steps are required."}};
    } else {
      result = {{"status", "ok"},
                {"pipeline_id", id},
                {"name", args.value("name", "")}};
    }
  } else if (operation == "list_pipelines") {
    auto pipelines = pipeline_executor_->ListPipelines();
    result = {{"status", "ok"},
              {"pipelines", pipelines},
              {"count", static_cast<int>(pipelines.size())}};
  } else if (operation == "run_pipeline") {
    std::string pid = args.value("pipeline_id", "");
    if (pid.empty()) {
      result = {{"error", "pipeline_id is required"}};
    } else {
      nlohmann::json input_vars =
          args.value("input_vars", nlohmann::json::object());
      auto run_result = pipeline_executor_->RunPipeline(pid, input_vars);

      nlohmann::json steps_json = nlohmann::json::array();
      for (auto& [step_id, step_result] : run_result.step_results) {
        steps_json.push_back(
            {{"step_id", step_id},
             {"result", step_result.substr(
                            0, std::min((size_t)500, step_result.size()))}});
      }

      result = {{"status", run_result.status},
                {"pipeline_id", run_result.pipeline_id},
                {"duration_ms", run_result.duration_ms},
                {"steps", steps_json}};
    }
  } else if (operation == "delete_pipeline") {
    std::string pid = args.value("pipeline_id", "");
    if (pid.empty()) {
      result = {{"error", "pipeline_id is required"}};
    } else {
      bool ok = pipeline_executor_->DeletePipeline(pid);
      if (ok) {
        result = {{"status", "ok"}, {"deleted", pid}};
      } else {
        result = {{"error", "Pipeline not found: " + pid}};
      }
    }
  } else {
    result = {{"error", "Unknown pipeline operation: " + operation}};
  }

  return result.dump();
}

std::string AgentCore::ExecuteWorkflowOp(
    const std::string& operation,
    const nlohmann::json& args,
    const std::string& session_id) {
  (void)session_id;  // Reserved for future use
  if (!workflow_engine_) {
    return "{\"error\": "
           "\"WorkflowEngine not available\"}";
  }

  nlohmann::json result;

  if (operation == "create_workflow") {
    std::string md =
        args.value("markdown", "");
    if (md.empty()) {
      result = {{"error",
                 "markdown content is required"}};
    } else {
      std::string id =
          workflow_engine_->CreateWorkflow(md);
      if (id.empty()) {
        result = {{"error",
                   "Failed to create workflow. "
                   "name and steps are required "
                   "in markdown."}};
      } else {
        result = {{"status", "ok"},
                  {"workflow_id", id}};
      }
    }
  } else if (operation == "list_workflows") {
    auto workflows =
        workflow_engine_->ListWorkflows();
    result = {
        {"status", "ok"},
        {"workflows", workflows},
        {"count",
         static_cast<int>(workflows.size())}};
  } else if (operation == "run_workflow") {
    std::string wid =
        args.value("workflow_id", "");
    if (wid.empty()) {
      result = {{"error",
                 "workflow_id is required"}};
    } else {
      nlohmann::json input_vars = args.value(
          "input_vars",
          nlohmann::json::object());
      auto run_result =
          workflow_engine_->RunWorkflow(
              wid, input_vars);

      nlohmann::json steps_json =
          nlohmann::json::array();
      for (const auto& [step_id, step_result] :
           run_result.step_results) {
        steps_json.push_back(
            {{"step_id", step_id},
             {"result",
              step_result.substr(
                  0, std::min((size_t)500,
                              step_result.size()))}});
      }

      result = {
          {"status", run_result.status},
          {"workflow_id",
           run_result.workflow_id},
          {"duration_ms",
           run_result.duration_ms},
          {"steps", steps_json}};
    }
  } else if (operation == "delete_workflow") {
    std::string wid =
        args.value("workflow_id", "");
    if (wid.empty()) {
      result = {{"error",
                 "workflow_id is required"}};
    } else {
      bool ok =
          workflow_engine_->DeleteWorkflow(wid);
      if (ok) {
        result = {{"status", "ok"},
                  {"deleted", wid}};
      } else {
        result = {{"error",
                   "Workflow not found: " + wid}};
      }
    }
  } else {
    result = {{"error",
               "Unknown workflow operation: " +
                   operation}};
  }

  return result.dump();
}

std::string AgentCore::ExecuteActionOp(const std::string& operation,
                                       const nlohmann::json& args) {
  if (!action_bridge_) {
    return "{\"error\":"
           "\"Action bridge not available\"}";
  }

  if (operation == "execute_action") {
    std::string name = args.value("name", "");
    nlohmann::json arguments =
        args.value("arguments", nlohmann::json::object());
    if (name.empty()) {
      return "{\"error\":"
             "\"Action name is required\"}";
    }
    LOG(INFO) << "Executing Tizen action: " << name;
    return action_bridge_->ExecuteAction(name, arguments);
  }

  // Per-action tool: action_<name>
  if (operation.starts_with("action_")) {
    std::string name = operation.substr(7);  // skip "action_"
    LOG(INFO) << "Executing Tizen action (tool): " << name;
    return action_bridge_->ExecuteAction(name, args);
  }

  return "{\"error\":\"Unknown action op: " + operation + "\"}";
}

bool AgentCore::ConnectMcpServers(const std::string& config_path) {
  if (mcp_client_manager_) {
    bool ok = mcp_client_manager_->LoadConfigAndConnect(config_path);
    if (ok) ReloadSkills();
    return ok;
  }
  return false;
}

nlohmann::json AgentCore::GetMcpToolsJson() {
  nlohmann::json result = nlohmann::json::object();
  result["enabled"] = (mcp_client_manager_ != nullptr);
  auto arr = nlohmann::json::array();
  
  if (mcp_client_manager_) {
    auto tools = mcp_client_manager_->GetToolDeclarations();
    for (const auto& t : tools) {
      nlohmann::json tj;
      tj["name"] = t.name;
      tj["description"] = t.description;
      tj["parameters"] = t.parameters;
      arr.push_back(tj);
    }
  }
  result["tools"] = arr;
  result["tool_count"] = arr.size();
  return result;
}

std::string AgentCore::GenerateToolDoc(
    const std::string& tool_name,
    const std::string& binary_path,
    const std::string& help_output) {
  if (help_output.empty()) {
    // Fallback: minimal doc without LLM
    return "# " + tool_name +
           "\n\nSystem CLI tool.\n\n"
           "**Binary**: `" + binary_path +
           "`\n**Category**: system_cli\n";
  }

  std::lock_guard<std::mutex> lock(backend_mutex_);
  if (!backend_) {
    LOG(WARNING) << "GenerateToolDoc: "
                 << "no LLM backend available";
    return "";
  }

  // Build a one-shot prompt to generate
  // structured tool documentation optimized
  // for LLM CLI invocation
  std::string sys_prompt =
      "You are a tool documentation generator "
      "for an AI agent system called TizenClaw. "
      "Your output will be read by an LLM that "
      "needs to invoke this CLI tool with correct "
      "arguments.\n\n"
      "Generate a concise Markdown tool.md that "
      "an LLM can use to construct correct CLI "
      "invocations. Follow this EXACT format:\n\n"
      "```\n"
      "# <tool_name>\n"
      "**Description**: <one-line summary of "
      "what this tool does and when to use it>\n"
      "**Binary**: `<full_path>`\n"
      "**Category**: system_cli\n"
      "## Subcommands\n"
      "| Subcommand | Options |\n"
      "|---|---|\n"
      "| `<cmd>` | <description or options> |\n"
      "## Usage\n"
      "```\n"
      "<tool_name> <subcommand> [options]\n"
      "```\n"
      "## Output\n"
      "<describe output format: text, JSON, etc>\n"
      "```\n\n"
      "REFERENCE EXAMPLE (this is what good "
      "output looks like):\n"
      "```\n"
      "# tizen-app-manager-cli\n"
      "**Description**: Manage applications: "
      "list, terminate, launch via app_control, "
      "query package info.\n"
      "**Binary**: `/usr/bin/tizen-app-manager-cli`\n"
      "**Category**: system_cli\n"
      "## Subcommands\n"
      "| Subcommand | Options |\n"
      "|---|---|\n"
      "| `list` | List installed UI apps |\n"
      "| `terminate` | `--app-id <id>` |\n"
      "| `launch` | `--app-id <id> "
      "[--operation <op>] [--uri <uri>]` |\n"
      "| `package-info` | `--package-id <id>` |\n"
      "```\n\n"
      "RULES:\n"
      "- Output ONLY the Markdown, nothing else\n"
      "- Keep it CONCISE - under 80 lines total\n"
      "- Each subcommand row must show the exact "
      "CLI syntax with options/arguments\n"
      "- Group related commands if there are "
      "many (use ### subsection headings)\n"
      "- Use backticks for arguments: "
      "`<appid>`, `<pkgid>`\n"
      "- Include 3-5 Usage examples showing "
      "exact command lines the LLM should use\n"
      "- Describe output format (text/JSON/table). "
      "If actual execution outputs are provided, "
      "include brief snippets of them to illustrate the format.\n"
      "- Do NOT include raw help output\n"
      "- Do NOT wrap the entire output in "
      "code fences";

  std::string user_prompt =
      "Analyze this CLI tool's help and execution "
      "outputs, then generate a tool.md "
      "optimized for LLM CLI invocation.\n\n"
      "Tool: " + tool_name + "\n"
      "Binary: " + binary_path + "\n\n"
      "Outputs:\n```\n" +
      help_output + "\n```";

  std::vector<LlmMessage> messages;

  LlmMessage user_msg;
  user_msg.role = "user";
  user_msg.text = user_prompt;
  messages.push_back(std::move(user_msg));

  LOG(INFO) << "GenerateToolDoc: calling LLM for "
            << tool_name;

  try {
    auto response = backend_->Chat(
        messages, {}, nullptr, sys_prompt);
    if (!response.text.empty()) {
      LOG(INFO)
          << "GenerateToolDoc: LLM generated "
          << response.text.size() << " bytes";
      return response.text;
    }
    LOG(WARNING)
        << "GenerateToolDoc: LLM returned empty"
        << " success=" << response.success
        << " http=" << response.http_status
        << " err=" << response.error_message;
  } catch (const std::exception& e) {
    LOG(ERROR)
        << "GenerateToolDoc: LLM error: "
        << e.what();
  }

  return "";
}

std::string AgentCore::ExecuteCustomSkillOp(const nlohmann::json& args) {
  namespace fs = std::filesystem;
  LOG(INFO) << "CustomSkillOp";

  std::string op = args.value("operation", "");
  std::string name = args.value("skill_name", "");
  std::string desc = args.value("description", "");
  // Support both "code" and legacy "python_code"
  std::string code = args.value("code", "");
  if (code.empty()) {
    code = args.value("python_code", "");
  }
  std::string runtime = args.value("runtime", "python");
  std::string language = args.value("language", "");
  std::string risk = args.value("risk_level", "low");
  std::string category =
      args.value("category", "Uncategorized");
  nlohmann::json params_schema =
      args.value("parameters_schema",
                 nlohmann::json::object());

  const std::string base_dir =
      "/opt/usr/share/tizen-tools/"
      "custom_skills";

  // Ensure base directory exists
  std::error_code ec;
  fs::create_directories(base_dir, ec);

  if (op == "list") {
    nlohmann::json skills = nlohmann::json::array();
    if (fs::is_directory(base_dir, ec)) {
      for (const auto& entry : fs::directory_iterator(base_dir, ec)) {
        if (!entry.is_directory()) continue;
        auto dirname = entry.path().filename().string();
        std::string mpath = entry.path() / "manifest.json";
        std::ifstream mf(mpath);
        if (mf.is_open()) {
          try {
            nlohmann::json j;
            mf >> j;
            skills.push_back(
                {{"name", j.value("name", dirname)},
                 {"category",
                  j.value("category",
                          "Uncategorized")},
                 {"description",
                  j.value("description", "")},
                 {"runtime",
                  j.value("runtime", "python")},
                 {"verified",
                  j.value("verified", true)},
                 {"risk_level",
                  j.value("risk_level",
                          "low")}});
          } catch (const std::exception& e) {
            skills.push_back(
                {{"name", dirname},
                 {"error", e.what()}});
          }
        }
      }
    }
    return nlohmann::json({{"status", "ok"},
                           {"custom_skills", skills},
                           {"count", (int)skills.size()},
                           {"base_dir", base_dir}})
        .dump();
  }

  if (name.empty()) {
    return "{\"error\": \"skill_name "
           "is required\"}";
  }

  // Validate name (alphanumeric + underscore)
  for (char c : name) {
    if (!std::isalnum(c) && c != '_') {
      return "{\"error\": \"skill_name must "
             "be alphanumeric + underscore\"}";
    }
  }

  std::string skill_dir = base_dir + "/" + name;

  if (op == "create" || op == "update") {
    if (desc.empty() || code.empty()) {
      return "{\"error\": \"description and "
             "code are required for "
             "create/update\"}";
    }

    // Validate runtime
    if (runtime != "python" && runtime != "node" &&
        runtime != "native") {
      return "{\"error\": \"Invalid runtime: " +
             runtime +
             ". Must be python, node, "
             "or native\"}";
    }

    if (op == "create" && fs::is_directory(skill_dir, ec)) {
      return "{\"error\": \"Skill already "
             "exists: " +
             name + "\"}";
    }

    fs::create_directories(skill_dir, ec);

    // Build parameters schema if not provided
    if (params_schema.empty() || !params_schema.contains("type")) {
      params_schema = {{"type", "object"},
                       {"properties", nlohmann::json::object()},
                       {"required", nlohmann::json::array()}};
    }

    // Determine entry point based on runtime
    std::string ext;
    if (runtime == "python") ext = ".py";
    else if (runtime == "node") ext = ".js";
    else ext = "";  // native
    std::string entry_point = name + ext;

    // Write manifest.json
    nlohmann::json manifest = {{"name", name},
                               {"runtime", runtime},
                               {"category", category},
                               {"risk_level", risk},
                               {"description", desc},
                               {"entry_point", entry_point},
                               {"parameters",
                                params_schema}};
    if (runtime == "native" && !language.empty()) {
      manifest["language"] = language;
    }

    std::string manifest_path = skill_dir + "/manifest.json";
    std::ofstream mf(manifest_path);
    if (!mf.is_open()) {
      return "{\"error\": \"Failed to "
             "write manifest\"}";
    }
    mf << manifest.dump(4) << std::endl;
    mf.close();

    // Also generate SKILL.md (Anthropic standard)
    std::string skill_md_path =
        skill_dir + "/SKILL.md";
    std::ofstream sf(skill_md_path);
    if (sf.is_open()) {
      sf << SkillManifest::GenerateSkillMd(manifest);
      sf.close();
    }

    // Write code file
    std::string code_path =
        skill_dir + "/" + entry_point;
    if (runtime == "native") {
      // Native: base64-decode binary
      // For now, write raw (RPK will provide
      // pre-compiled binaries)
      std::ofstream bf(
          code_path, std::ios::binary);
      if (!bf.is_open()) {
        return "{\"error\": \"Failed to "
               "write binary file\"}";
      }
      bf << code;
      bf.close();
      // Set execute permission
      chmod(code_path.c_str(), 0755);
    } else {
      std::ofstream cf(code_path);
      if (!cf.is_open()) {
        return "{\"error\": \"Failed to "
               "write code file\"}";
      }
      cf << code;
      cf.close();
    }

    // Run verification
    auto verify_result =
        SkillVerifier::Verify(skill_dir);
    if (!verify_result.passed) {
      SkillVerifier::DisableSkill(skill_dir);
      std::string err_msg = "Verification failed: ";
      for (const auto& e : verify_result.errors) {
        err_msg += e + "; ";
      }
      LOG(WARNING) << err_msg;
      // Still return success but with warning
      cached_tools_loaded_.store(false);
      return nlohmann::json(
                 {{"status", "warning"},
                  {"operation", op},
                  {"skill_name", name},
                  {"verified", false},
                  {"errors", verify_result.errors},
                  {"message",
                   "Skill " + name +
                       " created but disabled "
                       "due to verification failure"}})
          .dump();
    }

    SkillVerifier::EnableSkill(skill_dir);

    // Invalidate tool cache so new skill
    // is discovered on next prompt
    cached_tools_loaded_.store(false);

    return nlohmann::json(
               {{"status", "ok"},
                {"operation", op},
                {"skill_name", name},
                {"runtime", runtime},
                {"verified", true},
                {"code_path", code_path},
                {"message", "Skill " + name +
                                (op == "create" ? " created" : " updated") +
                                " and verified"}})
        .dump();

  } else if (op == "delete") {
    if (!fs::is_directory(skill_dir, ec)) {
      return "{\"error\": \"Skill not found: " + name + "\"}";
    }

    fs::remove_all(skill_dir, ec);
    cached_tools_loaded_.store(false);

    return nlohmann::json({{"status", "ok"},
                           {"operation", "delete"},
                           {"skill_name", name},
                           {"message", "Skill " + name + " deleted"}})
        .dump();
  }

  return "{\"error\": \"Unknown operation: " + op + "\"}";
}

std::string AgentCore::ExecuteCli(const std::string& tool_name,
                                  const std::string& arguments) {
  LOG(INFO) << "ExecuteCli: tool=" << tool_name
            << " args=" << arguments;

  if (tool_name.empty()) {
    return "{\"error\": \"tool_name is required\"}";
  }

  // Check system CLI tools first (/usr/bin whitelist)
  auto& sys_cli = SystemCliAdapter::GetInstance();
  if (sys_cli.HasTool(tool_name)) {
    std::string validation = sys_cli.ValidateArguments(tool_name, arguments);
    if (!validation.empty()) {
      LOG(WARNING) << "SystemCli argument blocked: " << validation;
      return nlohmann::json({{"error", validation}}).dump();
    }

    LOG(INFO) << "Routing system CLI via tool-executor: " << tool_name;

    int timeout = sys_cli.GetTimeout(tool_name);
    std::string result = container_->ExecuteCliTool(
        tool_name, arguments, timeout);

    if (result.empty() || result == "{}") {
      LOG(ERROR) << "Tool executor unreachable for execute_cli, "
                 << "falling back to host popen";
      // Fallback: direct popen (e.g., tool-executor not running)
      std::string bin_path = sys_cli.Resolve(tool_name);
      std::string cmd = bin_path + " " + arguments + " 2>&1";
      std::string output;
      FILE* pipe = popen(cmd.c_str(), "r");
      if (!pipe) {
        return "{\"error\": \"Failed to execute system CLI tool\"}";
      }
      char buffer[4096];
      while (fgets(buffer, sizeof(buffer), pipe) != nullptr) {
        output += buffer;
      }
      int status = pclose(pipe);
      while (!output.empty() && output.back() == '\n') {
        output.pop_back();
      }
      try {
        auto j = nlohmann::json::parse(output);
        return j.dump();
      } catch (const nlohmann::json::exception&) {
        return nlohmann::json(
                   {{"tool", tool_name},
                    {"source", "system_cli"},
                    {"exit_code", WIFEXITED(status)
                                      ? WEXITSTATUS(status) : -1},
                    {"output", output}})
            .dump();
      }
    }

    return result;
  }

  // Resolve tool directory (TPK CLI plugins)
  std::string dir_name;
  {
    std::lock_guard<std::mutex> lock(tools_mutex_);
    auto it = cli_dirs_.find(tool_name);
    if (it != cli_dirs_.end()) {
      dir_name = it->second;
    } else {
      // Try exact directory name
      dir_name = tool_name;
    }
  }

  std::string cli_dir =
      std::string("/opt/usr/share/tizen-tools/cli/") + dir_name;

  // Try executable name matching directory name first (e.g., aurum-cli/aurum-cli),
  // then fall back to generic "executable" name for backward compatibility
  std::string exec_path = cli_dir + "/" + dir_name;
  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::exists(exec_path, ec)) {
    exec_path = cli_dir + "/executable";
  }
  if (!fs::exists(exec_path, ec)) {
    LOG(ERROR) << "CLI executable not found: " << cli_dir
               << "/{" << dir_name << ",executable}";
    return "{\"error\": \"CLI tool not found: " + tool_name + "\"}";
  }

  // 1st priority: Execute via tool-executor socket (avoids popen/fork issues)
  std::string full_cmd = exec_path + " " + arguments;
  std::string te_result = container_->ExecuteCliTool(
      exec_path, arguments, 10);
  if (!te_result.empty() && te_result != "{}") {
    LOG(INFO) << "CLI tool via tool-executor OK: " << tool_name;
    return te_result;
  }

  LOG(WARNING) << "Tool executor unavailable for CLI tool "
               << tool_name << ", falling back to popen";

  // 2nd priority: Direct popen fallback
  std::string cmd = exec_path + " " + arguments + " 2>&1";
  LOG(INFO) << "Executing CLI (popen): " << cmd;

  std::string output;
  FILE* pipe = popen(cmd.c_str(), "r");
  if (!pipe) {
    int err = errno;
    LOG(ERROR) << "popen failed for CLI tool: " << tool_name
               << " errno=" << err << " (" << strerror(err) << ")";
    return "{\"error\": \"Failed to execute CLI tool: "
           + std::string(strerror(err)) + "\"}";
  }

  char buffer[4096];
  while (fgets(buffer, sizeof(buffer), pipe) != nullptr) {
    output += buffer;
  }
  int status = pclose(pipe);

  // Trim trailing newline
  while (!output.empty() && output.back() == '\n') {
    output.pop_back();
  }

  if (WIFEXITED(status) && WEXITSTATUS(status) != 0) {
    LOG(WARNING) << "CLI tool " << tool_name
                 << " exited with code " << WEXITSTATUS(status);
  }

  // Try to parse as JSON; if it fails, wrap in JSON
  try {
    auto j = nlohmann::json::parse(output);
    return j.dump();
  } catch (const nlohmann::json::exception&) {
    return nlohmann::json(
               {{"tool", tool_name},
                {"exit_code", WIFEXITED(status)
                                  ? WEXITSTATUS(status) : -1},
                {"output", output}})
        .dump();
  }
}

}  // namespace tizenclaw

// clang-format off
// NOLINTBEGIN
// These are at the end of the file to avoid
// disrupting the existing code structure.
// clang-format on
// NOLINTEND

namespace tizenclaw {

void AgentCore::InitializeToolDispatcher() {
#ifdef TIZEN_FEATURE_CODE_GENERATOR
  tool_dispatch_["execute_code"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteCode(
            args.value("code", ""));
      };
#endif  // TIZEN_FEATURE_CODE_GENERATOR

  // file_manager removed — use tizen-file-manager-cli
  // via execute_cli instead

  tool_dispatch_["switch_user"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string& session_id) {
        std::string target_user = args.value("user_id", "");
        if (target_user.empty()) {
          return std::string("{\"error\": \"user_id is required\"}");
        }
        user_profile_store_.BindSession(session_id, target_user);
        return std::string("{\"status\": \"success\", \"message\": \"Switched to ") +
            target_user + "\"}";
      };

  for (const auto& n :
       {"create_task", "list_tasks",
        "cancel_task"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string&) {
          return ExecuteTaskOp(name, args);
        };
  }

  for (const auto& n :
       {"create_session", "list_sessions",
        "send_to_session"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string& sid) {
          return ExecuteSessionOp(
              name, args, sid);
        };
  }

#ifdef TIZEN_FEATURE_CODE_GENERATOR
  tool_dispatch_["manage_custom_skill"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteCustomSkillOp(args);
      };
#endif  // TIZEN_FEATURE_CODE_GENERATOR

  for (const auto& n :
       {"ingest_document",
        "search_knowledge"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string&) {
          return ExecuteRagOp(name, args);
        };
  }

  tool_dispatch_["lookup_web_api"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        std::string op =
            args.value("operation", "");
        return ExecuteWebApiLookup(op, args);
      };

  for (const auto& n :
       {"run_supervisor",
        "list_agent_roles",
        "get_agent_status",
        "list_agents"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string& sid) {
          return ExecuteSupervisorOp(
              name, args, sid);
        };
  }

  // Agent factory tools
  tool_dispatch_["spawn_agent"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        if (!agent_factory_) {
          return std::string(
              "{\"error\": \"AgentFactory "
              "not available\"}");
        }
        return agent_factory_->SpawnAgent(args);
      };

  tool_dispatch_["list_dynamic_agents"] =
      [this](const nlohmann::json&,
             const std::string&,
             const std::string&) {
        if (!agent_factory_) {
          return std::string(
              "{\"error\": \"AgentFactory "
              "not available\"}");
        }
        nlohmann::json result = {
            {"status", "ok"},
            {"agents",
             agent_factory_->ListDynamicAgents()}};
        return result.dump();
      };

  tool_dispatch_["remove_agent"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        if (!agent_factory_) {
          return std::string(
              "{\"error\": \"AgentFactory "
              "not available\"}");
        }
        std::string name =
            args.value("name", "");
        if (name.empty()) {
          return std::string(
              "{\"error\": \"name is "
              "required\"}");
        }
        return agent_factory_->RemoveAgent(name);
      };

  for (const auto& n :
       {"create_pipeline", "list_pipelines",
        "run_pipeline", "delete_pipeline"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string& sid) {
          return ExecutePipelineOp(
              name, args, sid);
        };
  }

  for (const auto& n :
       {"create_workflow", "list_workflows",
        "run_workflow", "delete_workflow"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string& sid) {
          return ExecuteWorkflowOp(
              name, args, sid);
        };
  }

  // Memory tools
  for (const auto& n :
       {"remember", "recall", "forget"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string&) {
          return ExecuteMemoryOp(name, args);
        };
  }

  // CLI tool
  tool_dispatch_["execute_cli"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteCli(
            args.value("tool_name", ""),
            args.value("arguments", ""));
      };

  for (const auto& n :
       {"start_cli_session", "send_to_cli",
        "read_cli_output", "close_cli_session"}) {
    tool_dispatch_[n] =
        [this](const nlohmann::json& args,
               const std::string& name,
               const std::string&) {
          std::string op;
          if (name == "start_cli_session") op = "start";
          else if (name == "send_to_cli") op = "send";
          else if (name == "read_cli_output") op = "read";
          else if (name == "close_cli_session") op = "close";
          return ExecuteCliSessionOp(op, args);
        };
  }

  // Dynamic web app generation
  tool_dispatch_["generate_web_app"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return GenerateWebApp(args);
      };

  // Register built-in tools to CapabilityRegistry
  auto& reg = CapabilityRegistry::GetInstance();
  auto register_builtin =
      [&](const std::string& name,
          const std::string& desc,
          const std::string& category,
          SideEffect se) {
        Capability cap;
        cap.name = name;
        cap.description = desc;
        cap.category = category;
        cap.source = CapabilitySource::kBuiltin;
        cap.contract.side_effect = se;
        cap.contract.execution_env = "host";
        reg.Register(name, cap);
      };

#ifdef TIZEN_FEATURE_CODE_GENERATOR
  register_builtin(
      "execute_code", "Execute Python code",
      "code_execution",
      SideEffect::kIrreversible);
#endif  // TIZEN_FEATURE_CODE_GENERATOR
  register_builtin(
      "create_task", "Create scheduled task",
      "scheduler", SideEffect::kReversible);
  register_builtin(
      "list_tasks", "List scheduled tasks",
      "scheduler", SideEffect::kNone);
  register_builtin(
      "cancel_task", "Cancel scheduled task",
      "scheduler", SideEffect::kReversible);
  register_builtin(
      "create_session", "Create agent session",
      "multi_agent", SideEffect::kReversible);
  register_builtin(
      "list_sessions", "List agent sessions",
      "multi_agent", SideEffect::kNone);
  register_builtin(
      "send_to_session", "Send to agent session",
      "multi_agent", SideEffect::kReversible);
  register_builtin(
      "ingest_document", "Ingest RAG document",
      "knowledge", SideEffect::kReversible);
  register_builtin(
      "search_knowledge", "Search knowledge base",
      "knowledge", SideEffect::kNone);
  register_builtin(
      "run_supervisor", "Run supervisor agent",
      "multi_agent", SideEffect::kReversible);
  register_builtin(
      "list_agent_roles", "List agent roles",
      "multi_agent", SideEffect::kNone);
  register_builtin(
      "list_agents",
      "List all running agents",
      "multi_agent", SideEffect::kNone);
  register_builtin(
      "spawn_agent", "Create dynamic agent",
      "multi_agent", SideEffect::kReversible);
  register_builtin(
      "create_workflow", "Create workflow",
      "workflow", SideEffect::kReversible);
  register_builtin(
      "list_workflows", "List workflows",
      "workflow", SideEffect::kNone);
  register_builtin(
      "run_workflow", "Run workflow",
      "workflow", SideEffect::kReversible);
  register_builtin(
      "delete_workflow", "Delete workflow",
      "workflow", SideEffect::kIrreversible);
  register_builtin(
      "remember", "Store memory",
      "memory", SideEffect::kReversible);
  register_builtin(
      "recall", "Recall memory",
      "memory", SideEffect::kNone);
  register_builtin(
      "forget", "Forget memory",
      "memory", SideEffect::kIrreversible);
  register_builtin(
      "execute_cli", "Execute CLI tool",
      "cli", SideEffect::kReversible);
  register_builtin(
      "generate_web_app",
      "Generate dynamic web app",
      "web_app", SideEffect::kReversible);

  LOG(INFO) << "Tool dispatcher initialized ("
            << tool_dispatch_.size()
            << " handlers, "
            << reg.Size()
            << " capabilities)";
}

std::string AgentCore::ExecuteMemoryOp(
    const std::string& operation,
    const nlohmann::json& args) {
  if (operation == "remember") {
    MemoryEntry entry;
    std::string type_str =
        args.value("type", "long-term");
    entry.type = (type_str == "episodic")
                     ? MemoryType::kEpisodic
                     : MemoryType::kLongTerm;
    entry.title = args.value("title", "");
    entry.content = args.value("content", "");
    entry.importance =
        args.value("importance", "medium");

    if (args.contains("tags") &&
        args["tags"].is_array()) {
      for (const auto& t : args["tags"])
        entry.tags.push_back(
            t.get<std::string>());
    }

    if (entry.title.empty()) {
      return "{\"error\": "
             "\"title is required\"}";
    }

    bool ok = memory_store_.WriteMemory(entry);
    return ok
               ? nlohmann::json(
                     {{"status", "saved"},
                      {"type", type_str},
                      {"title", entry.title}})
                     .dump()
               : "{\"error\": "
                 "\"write failed\"}";
  }

  if (operation == "recall") {
    std::string keyword =
        args.value("keyword", "");
    std::string type_str =
        args.value("type", "all");

    nlohmann::json results =
        nlohmann::json::array();

    auto search_type =
        [&](MemoryType mt) {
          auto files =
              memory_store_.ListMemories(mt);
          for (const auto& f : files) {
            if (!keyword.empty() &&
                f.find(keyword) ==
                    std::string::npos) {
              // Check content too
              auto entry =
                  memory_store_.ReadMemory(mt, f);
              if (!entry) continue;
              if (entry->title.find(keyword) ==
                      std::string::npos &&
                  entry->content.find(keyword) ==
                      std::string::npos)
                continue;
              results.push_back(
                  {{"type",
                    MemoryStore::
                        TypeToString(mt) +
                        ""},
                   {"filename", f},
                   {"title", entry->title},
                   {"content",
                    entry->content}});
            } else {
              auto entry =
                  memory_store_.ReadMemory(mt, f);
              if (!entry) continue;
              results.push_back(
                  {{"type",
                    MemoryStore::
                        TypeToString(mt) +
                        ""},
                   {"filename", f},
                   {"title", entry->title},
                   {"content",
                    entry->content}});
            }
          }
        };

    if (type_str == "long-term" ||
        type_str == "all")
      search_type(MemoryType::kLongTerm);
    if (type_str == "episodic" ||
        type_str == "all")
      search_type(MemoryType::kEpisodic);

    return nlohmann::json(
               {{"results", results},
                {"count", results.size()}})
        .dump();
  }

  if (operation == "forget") {
    std::string type_str =
        args.value("type", "");
    std::string filename =
        args.value("filename", "");

    if (type_str.empty() || filename.empty()) {
      return "{\"error\": "
             "\"type and filename "
             "are required\"}";
    }

    MemoryType mt =
        (type_str == "episodic")
            ? MemoryType::kEpisodic
            : MemoryType::kLongTerm;

    bool ok =
        memory_store_.DeleteMemory(mt, filename);
    return ok
               ? nlohmann::json(
                     {{"status", "deleted"},
                      {"filename", filename}})
                     .dump()
               : "{\"error\": "
                 "\"not found\"}";
  }

  return "{\"error\": \"Unknown memory "
         "operation: " +
         operation + "\"}";
}

std::string AgentCore::GenerateWebApp(
    const nlohmann::json& args) {
  namespace fs = std::filesystem;

  std::string app_id = args.value("app_id", "");
  std::string title = args.value("title", "");
  std::string html = args.value("html", "");
  std::string css = args.value("css", "");
  std::string js = args.value("js", "");

  // Validate app_id
  if (app_id.empty() || app_id.size() > 64) {
    return "{\"error\": \"app_id is required "
           "(max 64 chars)\"}";
  }
  for (char c : app_id) {
    if (!std::isalnum(c) && c != '_') {
      return "{\"error\": \"app_id must be "
             "lowercase alphanumeric + "
             "underscore only\"}";
    }
  }

  if (html.empty()) {
    return "{\"error\": \"html is required\"}";
  }

  // Create app directory
  std::string apps_dir =
      std::string(APP_DATA_DIR) + "/web/apps";
  std::string app_dir = apps_dir + "/" + app_id;
  std::error_code ec;
  fs::create_directories(app_dir, ec);
  if (ec) {
    return "{\"error\": \"Failed to create "
           "app directory: " +
           ec.message() + "\"}";
  }

  // Write index.html
  {
    std::ofstream f(app_dir + "/index.html");
    if (!f.is_open()) {
      return "{\"error\": \"Failed to write "
             "index.html\"}";
    }
    f << html;
  }

  // Write optional style.css
  if (!css.empty()) {
    std::ofstream f(app_dir + "/style.css");
    if (f.is_open()) f << css;
  }

  // Write optional app.js
  if (!js.empty()) {
    std::ofstream f(app_dir + "/app.js");
    if (f.is_open()) f << js;
  }

  // Download optional assets
  nlohmann::json downloaded_assets =
      nlohmann::json::array();
  if (args.contains("assets") &&
      args["assets"].is_array()) {
    for (const auto& asset : args["assets"]) {
      std::string url =
          asset.value("url", "");
      std::string filename =
          asset.value("filename", "");

      if (url.empty() || filename.empty())
        continue;

      // Prevent path traversal in filename
      if (filename.find("..") !=
              std::string::npos ||
          filename.find('/') !=
              std::string::npos ||
          filename.find('\\') !=
              std::string::npos) {
        LOG(WARNING) << "GenerateWebApp: "
                     << "skipping unsafe asset "
                     << "filename: " << filename;
        continue;
      }

      std::string asset_path =
          app_dir + "/" + filename;

      // Download using libcurl
      CURL* curl = curl_easy_init();
      if (!curl) continue;

      FILE* fp = fopen(asset_path.c_str(), "wb");
      if (!fp) {
        curl_easy_cleanup(curl);
        continue;
      }

      curl_easy_setopt(
          curl, CURLOPT_URL, url.c_str());
      curl_easy_setopt(
          curl, CURLOPT_WRITEDATA, fp);
      curl_easy_setopt(
          curl, CURLOPT_FOLLOWLOCATION, 1L);
      curl_easy_setopt(
          curl, CURLOPT_TIMEOUT, 30L);
      curl_easy_setopt(
          curl, CURLOPT_MAXFILESIZE,
          10 * 1024 * 1024L);  // 10MB limit

      CURLcode res = curl_easy_perform(curl);
      fclose(fp);

      if (res == CURLE_OK) {
        downloaded_assets.push_back(
            {{"filename", filename},
             {"status", "ok"}});
        LOG(INFO) << "GenerateWebApp: downloaded "
                  << "asset: " << filename;
      } else {
        // Remove failed download
        fs::remove(asset_path, ec);
        downloaded_assets.push_back(
            {{"filename", filename},
             {"status", "failed"},
             {"error",
              curl_easy_strerror(res)}});
        LOG(WARNING) << "GenerateWebApp: asset "
                     << "download failed: "
                     << filename << " - "
                     << curl_easy_strerror(res);
      }
      curl_easy_cleanup(curl);
    }
  }

  // Write manifest.json
  {
    auto now =
        std::chrono::system_clock::now();
    auto epoch =
        std::chrono::duration_cast<
            std::chrono::seconds>(
            now.time_since_epoch())
            .count();

    nlohmann::json manifest = {
        {"app_id", app_id},
        {"title", title},
        {"created_at", epoch},
        {"has_css", !css.empty()},
        {"has_js", !js.empty()},
        {"assets", downloaded_assets}};

    // Store allowed_tools for Bridge API
    if (args.contains("allowed_tools") &&
        args["allowed_tools"].is_array()) {
      manifest["allowed_tools"] =
          args["allowed_tools"];
    }

    std::ofstream mf(
        app_dir + "/manifest.json");
    if (mf.is_open()) mf << manifest.dump(2);
  }

  LOG(INFO) << "GenerateWebApp: created '"
            << app_id << "' at " << app_dir;

  std::string app_url =
      "http://localhost:9090/apps/" + app_id + "/";

  // Auto-launch bridge app to display the generated app
  bool launched = LaunchBridgeApp(app_id);

  nlohmann::json result = {
      {"status", "ok"},
      {"app_id", app_id},
      {"title", title},
      {"url", "/apps/" + app_id + "/"},
      {"webview_launched", launched},
      {"message",
       "Web app created. Access at " +
           app_url}};

  if (!downloaded_assets.empty()) {
    result["assets"] = downloaded_assets;
  }

  return result.dump();
}

bool AgentCore::LaunchBridgeApp(
    const std::string& app_id) {
  constexpr const char* kBridgeAppId =
      "QvaPeQ7RDA.tizenclawbridge";
  constexpr const char* kWebviewAppId =
      "org.tizen.tizenclaw-webview";
  std::string app_url =
      "http://localhost:9090/apps/" + app_id + "/";

  // Try launching with url key via bundle
  bundle* b = bundle_create();
  if (b) {
    bundle_add_str(b, "url", app_url.c_str());
    int ret = aul_launch_app(kBridgeAppId, b);
    bundle_free(b);
    if (ret >= 0) {
      LOG(INFO) << "LaunchBridgeApp: launched "
                << kBridgeAppId
                << " with url=" << app_url;
      return true;
    }
    LOG(WARNING) << "LaunchBridgeApp: launch "
                 << "failed (ret=" << ret << ")";
  }

  // Fallback: try plain open
  int ret = aul_open_app(kBridgeAppId);
  if (ret >= 0) {
    LOG(INFO) << "LaunchBridgeApp: opened "
              << kBridgeAppId
              << " (without url param)";
    return true;
  }
  LOG(WARNING) << "LaunchBridgeApp: "
               << kBridgeAppId
               << " not available (ret="
               << ret << "), trying webview";

  // Fallback: try tizenclaw-webview with
  // __APP_SVC_URI__ key
  bundle* wb = bundle_create();
  if (wb) {
    bundle_add_str(
        wb, "__APP_SVC_URI__", app_url.c_str());
    int wret = aul_launch_app(kWebviewAppId, wb);
    bundle_free(wb);
    if (wret >= 0) {
      LOG(INFO) << "LaunchBridgeApp: launched "
                << kWebviewAppId
                << " with URI=" << app_url;
      return true;
    }
    LOG(WARNING) << "LaunchBridgeApp: "
                 << kWebviewAppId
                 << " launch failed (ret="
                 << wret << ")";
  }

  LOG(WARNING) << "LaunchBridgeApp: "
               << "no webview app available";
  return false;
}

std::string AgentCore::ExecuteBridgeTool(
    const std::string& tool_name,
    const nlohmann::json& args,
    const std::vector<std::string>&
        allowed_tools) {
  // Validate against allowlist
  if (!allowed_tools.empty()) {
    bool found = false;
    for (const auto& t : allowed_tools) {
      if (t == tool_name) {
        found = true;
        break;
      }
    }
    if (!found) {
      LOG(WARNING) << "Bridge: tool '"
                   << tool_name
                   << "' not in allowed_tools";
      return "{\"error\": \"Tool not allowed "
             "for this app: " +
             tool_name + "\"}";
    }
  }

  // Check risk level — block high-risk tools
  // from direct bridge access
  RiskLevel risk =
      tool_policy_.GetRiskLevel(tool_name);
  if (risk == RiskLevel::kHigh) {
    LOG(WARNING) << "Bridge: blocked high-risk "
                 << "tool: " << tool_name;
    return "{\"error\": \"High-risk tool "
           "not available via bridge: " +
           tool_name + "\"}";
  }

  // Dispatch via the same tool_dispatch_ map
  auto it = tool_dispatch_.find(tool_name);
  if (it != tool_dispatch_.end()) {
    return it->second(
        args, tool_name, "bridge");
  }

  // Try action tools
  if (tool_name == "execute_action" ||
      tool_name.starts_with("action_")) {
    return ExecuteActionOp(tool_name, args);
  }

  // Try skill execution (container)
  return ExecuteSkill(tool_name, args);
}

std::vector<LlmToolDecl>
AgentCore::GetToolDeclarations() const {
  std::lock_guard<std::mutex> lock(tools_mutex_);
  return cached_tools_;
}

}  // namespace tizenclaw
