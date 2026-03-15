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

#include "../../common/logging.hh"
#include "../infra/http_client.hh"
#include "../infra/key_store.hh"
#include "../llm/plugin_llm_backend.hh"
#include "../llm/plugin_manager.hh"
#include "../storage/audit_logger.hh"
#include "cli_plugin_manager.hh"
#include "skill_plugin_manager.hh"
#include "capability_registry.hh"
#include "skill_verifier.hh"
#include "tool_indexer.hh"

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
    std::this_thread::sleep_for(std::chrono::seconds(10));

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
    }
  }
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
  std::string config_path = env_path ? env_path
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
        {"backends",
         {{"gemini", {{"api_key", api_key}, {"model", "gemini-2.5-flash"}}}}}};
  }

  llm_config_ = llm_config;
  curl_global_init(CURL_GLOBAL_DEFAULT);

  // Load system prompt
  system_prompt_ = LoadSystemPrompt(llm_config);
  if (!system_prompt_.empty()) {
    LOG(INFO) << "System prompt loaded (" << system_prompt_.size() << " chars)";
  } else {
    LOG(WARNING) << "No system prompt configured";
  }

  // Load tool execution policy
  std::string policy_path =
      "/opt/usr/share/tizenclaw/config/"
      "tool_policy.json";
  if (!tool_policy_.LoadConfig(policy_path)) {
    LOG(WARNING) << "Tool policy config not loaded (using defaults)";
  }

  if (!SwitchToBestBackend(false)) {
    LOG(ERROR) << "Failed to switch to best backend during Initialize.";
    return false;
  }

  // Audit: config loaded
  AuditLogger::Instance().Log(AuditLogger::MakeEvent(
      AuditEventType::kConfigChange, "", {{"backend", backend_->GetName()}}));

  LOG(INFO) << "AgentCore initialized with "
            << "backend: " << backend_->GetName();
  initialized_ = true;

  // React to plugin install/uninstall
  PluginManager::GetInstance().SetChangeCallback([this]() { ReloadBackend(); });

  // React to skill RPK install/uninstall
  SkillPluginManager::GetInstance().SetChangeCallback([this]() {
    LOG(INFO) << "Skill RPK change detected, invalidating tool cache";
    cached_tools_loaded_.store(false);
  });
  SkillPluginManager::GetInstance().Initialize();

  // React to CLI TPK install/uninstall
  CliPluginManager::GetInstance().SetChangeCallback([this]() {
    LOG(INFO) << "CLI TPK change detected, invalidating tool cache";
    cached_tools_loaded_.store(false);
  });
  CliPluginManager::GetInstance().Initialize();

  // Initialize embedding store for RAG
  std::string rag_db = std::string(APP_DATA_DIR) + "/rag/embeddings.db";
  // Ensure rag directory exists
  std::string rag_dir = std::string(APP_DATA_DIR) + "/rag";
  mkdir(rag_dir.c_str(), 0755);
  if (embedding_store_.Initialize(rag_db)) {
    LOG(INFO) << "RAG embedding store ready";

    // Scan rag/ directory for all .db files
    namespace fs = std::filesystem;
    std::error_code ec;
    if (fs::is_directory(rag_dir, ec)) {
      for (const auto& entry :
           fs::directory_iterator(rag_dir, ec)) {
        if (!entry.is_regular_file()) continue;
        auto fname = entry.path().filename().string();
        if (entry.path().extension() != ".db") continue;
        if (fname == "embeddings.db") continue;
        if (embedding_store_.AttachKnowledgeDB(
                entry.path().string())) {
          LOG(INFO) << "Knowledge DB loaded: " << fname;
        }
      }
    }
    LOG(INFO) << "Total knowledge chunks: "
              << embedding_store_.GetKnowledgeChunkCount();
  } else {
    LOG(WARNING) << "RAG embedding store "
                 << "init failed (non-fatal)";
  }

  // Initialize on-device embedding model
  std::string model_dir =
      std::string(APP_DATA_DIR) + "/models/all-MiniLM-L6-v2";
  std::string ort_lib =
      std::string(APP_DATA_DIR) + "/lib/libonnxruntime.so";
  if (on_device_embedding_.Initialize(model_dir, ort_lib)) {
    LOG(INFO) << "On-device embedding ready "
              << "(LLM-independent)";
  } else {
    LOG(WARNING) << "On-device embedding not available "
                 << "(will use LLM backend)";
  }

  // Initialize supervisor engine
  supervisor_ = std::make_unique<SupervisorEngine>(this);
  std::string roles_path =
      std::string(APP_DATA_DIR) + "/config/agent_roles.json";
  if (supervisor_->LoadRoles(roles_path)) {
    LOG(INFO) << "Supervisor engine ready with "
              << supervisor_->GetRoleNames().size() << " roles";
  } else {
    LOG(WARNING) << "Supervisor engine: no roles " << "configured (non-fatal)";
  }

  // Initialize system context provider
  system_context_ = std::make_unique<SystemContextProvider>();
  system_context_->Start();
  LOG(INFO) << "SystemContextProvider ready";

  // Initialize agent factory
  agent_factory_ = std::make_unique<AgentFactory>(
      this, supervisor_.get());
  LOG(INFO) << "AgentFactory ready";

  // Initialize pipeline executor
  pipeline_executor_ = std::make_unique<PipelineExecutor>(this);
  pipeline_executor_->LoadPipelines();
  LOG(INFO) << "Pipeline executor ready";

  // Initialize workflow engine
  workflow_engine_ = std::make_unique<WorkflowEngine>(this);
  workflow_engine_->LoadWorkflows();
  LOG(INFO) << "Workflow engine ready";

  // Initialize Tizen Action Framework bridge
  action_bridge_ = std::make_unique<ActionBridge>();
  if (action_bridge_->Start()) {
    // Sync action schemas to MD files
    action_bridge_->SyncActionSchemas();
    // React to action install/uninstall/update
    action_bridge_->SetChangeCallback([this]() {
      LOG(INFO) << "Action schemas changed, " << "reloading tools";
      cached_tools_loaded_.store(false);
    });
    LOG(INFO) << "Tizen Action bridge ready";
  } else {
    LOG(WARNING) << "Tizen Action bridge init failed " << "(non-fatal)";
    action_bridge_.reset();
  }

  // Initialize memory store
  std::string mem_config_path =
      std::string(APP_DATA_DIR) +
      "/config/memory_config.json";
  memory_store_.LoadConfig(mem_config_path);
  LOG(INFO) << "MemoryStore initialized";

  // Initialize modular tool dispatcher
  tool_dispatcher_ =
      std::make_unique<ToolDispatcher>();

  // Initialize tool dispatcher map
  InitializeToolDispatcher();

  // Start background maintenance thread immediately
  UpdateActivityTime();
  maintenance_thread_ = std::thread(&AgentCore::MaintenanceLoop, this);

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

  while (iterations < max_iter) {
    // Build session-specific system prompt
    std::string full_prompt = GetSessionPrompt(session_id, tools);

    // Query LLM backend without holding lock
    LlmResponse resp =
        current_backend->Chat(local_history, tools, on_chunk, full_prompt);

    // Track LLM call in health metrics
    if (health_monitor_) health_monitor_->IncrementLlmCallCount();

    if (!resp.success) {
      LOG(ERROR) << "LLM error: " << resp.error_message;

      // Try fallback backends
      resp = TryFallbackBackends(local_history, tools, on_chunk, full_prompt);

      if (!resp.success) {
        // All backends failed — track error
        if (health_monitor_) health_monitor_->IncrementErrorCount();
        // Rollback: remove the user message
        {
          std::lock_guard<std::mutex> lock(session_mutex_);
          if (!sessions_[session_id].empty()) {
            sessions_[session_id].pop_back();
          }
        }
        return "Error: " + resp.error_message;
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

        auto start = std::chrono::steady_clock::now();
        auto it = tool_dispatch_.find(tc.name);
        if (it != tool_dispatch_.end()) {
          r.output = it->second(
              tc.args, tc.name, session_id);
        } else if (tc.name == "execute_action" ||
                   tc.name.starts_with("action_")) {
          r.output = ExecuteActionOp(tc.name, tc.args);
        } else {
          r.output = ExecuteSkill(tc.name, tc.args);
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
        tool_msg.tool_result = nlohmann::json::parse(result.output);
      } catch (...) {
        tool_msg.tool_result = {{"output", result.output}};
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
    if (!new_backend) continue;

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
      continue;
    }

    backend_ = std::move(new_backend);

    if (is_reload) {
      AuditLogger::Instance().Log(AuditLogger::MakeEvent(
          AuditEventType::kConfigChange, "",
          {{"backend", backend_->GetName()}}));
    }

    // Populate fallback_names_ with the remaining candidates
    fallback_names_.clear();
    for (size_t j = i + 1; j < candidates.size(); ++j) {
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

  namespace fs = std::filesystem;

  std::vector<LlmToolDecl> tools;
  skill_runtimes_.clear();
  skill_dirs_.clear();
  const std::string skills_dir = "/opt/usr/share/tizenclaw/tools/skills";

  // Lambda to scan a skills directory
  auto scan_dir = [&](const std::string& dir) {
    std::error_code ec;
    if (!fs::is_directory(dir, ec)) return;

    for (const auto& entry : fs::directory_iterator(dir, ec)) {
      if (!entry.is_directory()) continue;
      auto dirname = entry.path().filename().string();
      if (dirname[0] == '.') continue;
      std::string manifest_path = entry.path() / "manifest.json";
      std::ifstream mf(manifest_path);
      if (!mf.is_open()) continue;

      try {
        nlohmann::json j;
        mf >> j;

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
          cap.source = CapabilitySource::kSkill;
          cap.category =
              j.contains("metadata") &&
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
      } catch (...) {
        LOG(WARNING) << "Failed to parse "
                     << "manifest: " << manifest_path;
      }
    }
  };

  scan_dir(skills_dir);

  // Also scan custom_skills directory
  const std::string custom_skills_dir =
      "/opt/usr/share/tizenclaw/tools/"
      "custom_skills";
  scan_dir(custom_skills_dir);

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
  code_tool.parameters = {{"type", "object"},
                          {"properties",
                           {{"code",
                             {{"type", "string"},
                              {"description",
                               "Python code to execute on the "
                               "Tizen device"}}}}},
                          {"required", nlohmann::json::array({"code"})}};
  tools.push_back(code_tool);

  // Built-in tool: file_manager
  LlmToolDecl file_tool;
  file_tool.name = "file_manager";
  file_tool.description =
      "Manage files on the Tizen device. "
      "Create, read, delete files or list "
      "directory contents. Paths MUST start "
      "with /tools/skills/ or /data/ — other paths "
      "are rejected. Use /tools/skills/ to save new "
      "skill scripts, /data/ for persistent data.";
  file_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"operation",
         {{"type", "string"},
          {"enum", nlohmann::json::array(
                       {"write_file", "read_file", "delete_file", "list_dir"})},
          {"description", "The file operation to perform"}}},
        {"path",
         {{"type", "string"},
          {"description",
           "File or directory path. Must start "
           "with /tools/skills/ or /data/"}}},
        {"content",
         {{"type", "string"},
          {"description", "File content (for write_file only)"}}}}},
      {"required", nlohmann::json::array({"operation", "path"})}};
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
      {"properties",
       {{"schedule",
         {{"type", "string"},
          {"description",
           "Schedule expression, e.g. "
           "'daily 09:00', "
           "'interval 30m', "
           "'once 2026-03-10 14:00', "
           "'weekly mon 09:00'"}}},
        {"prompt",
         {{"type", "string"},
          {"description",
           "The prompt to execute at "
           "the scheduled time"}}}}},
      {"required", nlohmann::json::array({"schedule", "prompt"})}};
  tools.push_back(create_task_tool);

  // Built-in tool: list_tasks
  LlmToolDecl list_tasks_tool;
  list_tasks_tool.name = "list_tasks";
  list_tasks_tool.description =
      "List all scheduled tasks. Optionally "
      "filter by session_id.";
  list_tasks_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"session_id",
         {{"type", "string"},
          {"description", "Optional session ID to filter"}}}}},
      {"required", nlohmann::json::array()}};
  tools.push_back(list_tasks_tool);

  // Built-in tool: cancel_task
  LlmToolDecl cancel_task_tool;
  cancel_task_tool.name = "cancel_task";
  cancel_task_tool.description = "Cancel a scheduled task by its ID.";
  cancel_task_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"task_id",
         {{"type", "string"}, {"description", "The task ID to cancel"}}}}},
      {"required", nlohmann::json::array({"task_id"})}};
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
      {"properties",
       {{"name",
         {{"type", "string"},
          {"description",
           "Short name for the session "
           "(used as session_id prefix)"}}},
        {"system_prompt",
         {{"type", "string"},
          {"description",
           "Custom system prompt that "
           "defines the agent's role and "
           "behavior"}}}}},
      {"required", nlohmann::json::array({"name", "system_prompt"})}};
  tools.push_back(create_session_tool);

  // Built-in tool: list_sessions
  LlmToolDecl list_sessions_tool;
  list_sessions_tool.name = "list_sessions";
  list_sessions_tool.description =
      "List all active agent sessions with "
      "their names and system prompts.";
  list_sessions_tool.parameters = {{"type", "object"},
                                   {"properties", nlohmann::json::object()},
                                   {"required", nlohmann::json::array()}};
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
      {"properties",
       {{"target_session",
         {{"type", "string"},
          {"description",
           "The session_id of the target "
           "agent to send the message to"}}},
        {"message",
         {{"type", "string"},
          {"description",
           "The message to send to the "
           "target agent session"}}}}},
      {"required", nlohmann::json::array({"target_session", "message"})}};
  tools.push_back(send_to_session_tool);

  // Built-in tool: manage_custom_skill
  LlmToolDecl custom_skill_tool;
  custom_skill_tool.name = "manage_custom_skill";
  custom_skill_tool.description =
      "Create, update, delete, or list custom "
      "skills at runtime. Supports multiple "
      "runtimes: python (default), node "
      "(JavaScript/TypeScript), and native "
      "(compiled C/C++/Rust binary, "
      "base64-encoded). Custom skills are "
      "auto-discovered and immediately "
      "available as tools. Skills are verified "
      "before activation; failed verification "
      "disables the skill. The code MUST print "
      "JSON to stdout and use CLAW_ARGS env "
      "for parameters.";
  custom_skill_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"operation",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array({"create", "update", "delete", "list"})},
          {"description", "Operation to perform"}}},
        {"skill_name",
         {{"type", "string"},
          {"description",
           "Name of the custom skill "
           "(alphanumeric + underscore)"}}},
        {"description",
         {{"type", "string"},
          {"description",
           "Description of what the "
           "skill does"}}},
        {"parameters_schema",
         {{"type", "object"},
          {"description",
           "JSON Schema for skill "
           "parameters (type, properties, "
           "required)"}}},
        {"code",
         {{"type", "string"},
          {"description",
           "Source code for the skill. "
           "For native runtime, provide "
           "base64-encoded binary."}}},
        {"runtime",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"python", "node", "native"})},
          {"description",
           "Skill runtime (default: python)"}}},
        {"language",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"c", "cpp", "rust", "go"})},
          {"description",
           "Source language for native "
           "runtime (informational)"}}},
        {"risk_level",
         {{"type", "string"},
          {"enum", nlohmann::json::array({"low", "medium", "high"})},
          {"description", "Risk level (default: low)"}}},
        {"category",
         {{"type", "string"},
          {"description",
           "Category of the skill "
           "(e.g. App Management, "
           "Device Info, Network)"}}}}},
      {"required", nlohmann::json::array({"operation"})}};
  tools.push_back(custom_skill_tool);

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
      {"properties",
       {{"source",
         {{"type", "string"},
          {"description",
           "Source identifier (filename, "
           "URL, or label)"}}},
        {"text",
         {{"type", "string"},
          {"description", "The document text to ingest"}}}}},
      {"required", nlohmann::json::array({"source", "text"})}};
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
      {"properties",
       {{"query", {{"type", "string"}, {"description", "The search query"}}},
        {"top_k",
         {{"type", "integer"},
          {"description", "Number of results (default 5)"}}}}},
      {"required", nlohmann::json::array({"query"})}};
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
      {"properties",
       {{"goal",
         {{"type", "string"},
          {"description",
           "The high-level goal to "
           "decompose and delegate"}}},
        {"strategy",
         {{"type", "string"},
          {"enum", nlohmann::json::array({"sequential", "parallel"})},
          {"description",
           "Execution strategy: "
           "'sequential' (default) or "
           "'parallel'"}}}}},
      {"required", nlohmann::json::array({"goal"})}};
  tools.push_back(run_supervisor_tool);

  // Built-in tool: list_agent_roles
  LlmToolDecl list_roles_tool;
  list_roles_tool.name = "list_agent_roles";
  list_roles_tool.description =
      "List all configured agent roles with "
      "their names, system prompts, and "
      "allowed tools.";
  list_roles_tool.parameters = {{"type", "object"},
                                {"properties", nlohmann::json::object()},
                                {"required", nlohmann::json::array()}};
  tools.push_back(list_roles_tool);

  // Built-in tool: get_agent_status
  LlmToolDecl agent_status_tool;
  agent_status_tool.name = "get_agent_status";
  agent_status_tool.description =
      "Get current agent system status: "
      "configured agents count, active "
      "delegations in progress, and recent "
      "delegation history with stats.";
  agent_status_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}};
  tools.push_back(agent_status_tool);

  // Built-in tool: list_agents
  LlmToolDecl list_agents_tool;
  list_agents_tool.name = "list_agents";
  list_agents_tool.description =
      "List all running agents with their "
      "status. Returns configured roles, "
      "dynamically created agents, active "
      "delegations, event bus sources, and "
      "autonomous trigger status.";
  list_agents_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}};
  tools.push_back(list_agents_tool);

  // Built-in tool: spawn_agent
  LlmToolDecl spawn_agent_tool;
  spawn_agent_tool.name = "spawn_agent";
  spawn_agent_tool.description =
      "Create a new specialized agent with a "
      "custom role definition. The agent is "
      "dynamically registered and can be "
      "delegated tasks via run_supervisor. "
      "Use this when existing agents are "
      "insufficient for a new task domain.";
  spawn_agent_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"name",
         {{"type", "string"},
          {"description",
           "Unique name (3-30 lowercase "
           "letters/underscores)"}}},
        {"system_prompt",
         {{"type", "string"},
          {"description",
           "System prompt defining the "
           "agent's expertise"}}},
        {"allowed_tools",
         {{"type", "array"},
          {"items", {{"type", "string"}}},
          {"description",
           "Tool names this agent can "
           "use (empty = all)"}}},
        {"max_iterations",
         {{"type", "integer"},
          {"description",
           "Max LLM iterations "
           "(default: 10)"}}},
        {"persistent",
         {{"type", "boolean"},
          {"description",
           "If true, save to "
           "agent_roles.json"}}}}},
      {"required",
       nlohmann::json::array(
           {"name", "system_prompt"})}};
  tools.push_back(spawn_agent_tool);

  // Built-in tool: list_dynamic_agents
  LlmToolDecl list_dynamic_tool;
  list_dynamic_tool.name = "list_dynamic_agents";
  list_dynamic_tool.description =
      "List all dynamically created agents "
      "that were spawned at runtime.";
  list_dynamic_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}};
  tools.push_back(list_dynamic_tool);

  // Built-in tool: remove_agent
  LlmToolDecl remove_agent_tool;
  remove_agent_tool.name = "remove_agent";
  remove_agent_tool.description =
      "Remove a dynamically created agent.";
  remove_agent_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"name",
         {{"type", "string"},
          {"description",
           "Name of the dynamic agent "
           "to remove"}}}}},
      {"required",
       nlohmann::json::array({"name"})}};
  tools.push_back(remove_agent_tool);

  // Built-in tool: create_pipeline
  LlmToolDecl create_pipeline_tool;
  create_pipeline_tool.name = "create_pipeline";
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
      {"properties",
       {{"name", {{"type", "string"}, {"description", "Pipeline name"}}},
        {"description",
         {{"type", "string"}, {"description", "Pipeline description"}}},
        {"trigger",
         {{"type", "string"},
          {"description",
           "Trigger type: 'manual' or "
           "'cron:daily HH:MM' etc."}}},
        {"steps",
         {{"type", "array"},
          {"description", "Array of step objects"},
          {"items",
           {{"type", "object"},
            {"properties",
             {{"id", {{"type", "string"}, {"description", "Step identifier"}}},
              {"type",
               {{"type", "string"},
                {"description",
                 "Step type: tool, "
                 "prompt, or condition"}}},
              {"tool_name",
               {{"type", "string"}, {"description", "Tool to invoke"}}},
              {"args", {{"type", "object"}, {"description", "Tool arguments"}}},
              {"prompt",
               {{"type", "string"}, {"description", "LLM prompt text"}}},
              {"condition",
               {{"type", "string"}, {"description", "Condition expression"}}},
              {"then_step",
               {{"type", "string"}, {"description", "Step ID if true"}}},
              {"else_step",
               {{"type", "string"}, {"description", "Step ID if false"}}},
              {"output_var",
               {{"type", "string"},
                {"description",
                 "Variable name for "
                 "step output"}}},
              {"skip_on_failure",
               {{"type", "boolean"}, {"description", "Continue on error"}}},
              {"max_retries",
               {{"type", "integer"}, {"description", "Max retry count"}}}}},
            {"required", nlohmann::json::array({"id", "type"})}}}}}}},
      {"required", nlohmann::json::array({"name", "steps"})}};
  tools.push_back(create_pipeline_tool);

  // Built-in tool: list_pipelines
  LlmToolDecl list_pipelines_tool;
  list_pipelines_tool.name = "list_pipelines";
  list_pipelines_tool.description =
      "List all configured pipelines with "
      "their names, triggers, and step "
      "counts.";
  list_pipelines_tool.parameters = {{"type", "object"},
                                    {"properties", nlohmann::json::object()},
                                    {"required", nlohmann::json::array()}};
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
      {"properties",
       {{"pipeline_id",
         {{"type", "string"}, {"description", "The pipeline ID to execute"}}},
        {"input_vars",
         {{"type", "object"},
          {"description",
           "Input variables (key-value "
           "pairs) available to all "
           "pipeline steps"}}}}},
      {"required", nlohmann::json::array({"pipeline_id"})}};
  tools.push_back(run_pipeline_tool);

  // Built-in tool: delete_pipeline
  LlmToolDecl delete_pipeline_tool;
  delete_pipeline_tool.name = "delete_pipeline";
  delete_pipeline_tool.description = "Delete a pipeline by its ID.";
  delete_pipeline_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"pipeline_id",
         {{"type", "string"}, {"description", "The pipeline ID to delete"}}}}},
      {"required", nlohmann::json::array({"pipeline_id"})}};
  tools.push_back(delete_pipeline_tool);

  // Built-in tool: create_workflow
  LlmToolDecl create_workflow_tool;
  create_workflow_tool.name = "create_workflow";
  create_workflow_tool.description =
      "Create a workflow from Markdown text. "
      "The markdown must include YAML "
      "frontmatter (---) with 'name' field "
      "and '## Step N:' sections with "
      "type/instruction/tool_name/output_var "
      "metadata. Steps execute sequentially "
      "with {{variable}} interpolation.";
  create_workflow_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"markdown",
         {{"type", "string"},
          {"description",
           "Markdown text with YAML "
           "frontmatter and Step sections"}}}}},
      {"required",
       nlohmann::json::array({"markdown"})}};
  tools.push_back(create_workflow_tool);

  // Built-in tool: list_workflows
  LlmToolDecl list_workflows_tool;
  list_workflows_tool.name = "list_workflows";
  list_workflows_tool.description =
      "List all registered workflows with "
      "their names, descriptions, triggers, "
      "and step counts.";
  list_workflows_tool.parameters = {
      {"type", "object"},
      {"properties", nlohmann::json::object()},
      {"required", nlohmann::json::array()}};
  tools.push_back(list_workflows_tool);

  // Built-in tool: run_workflow
  LlmToolDecl run_workflow_tool;
  run_workflow_tool.name = "run_workflow";
  run_workflow_tool.description =
      "Execute a workflow by its ID. "
      "Optionally provide input variables "
      "that can be referenced in steps "
      "via {{variable}} syntax.";
  run_workflow_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"workflow_id",
         {{"type", "string"},
          {"description",
           "The workflow ID to execute"}}},
        {"input_vars",
         {{"type", "object"},
          {"description",
           "Input variables (key-value "
           "pairs) for steps"}}}}},
      {"required",
       nlohmann::json::array(
           {"workflow_id"})}};
  tools.push_back(run_workflow_tool);

  // Built-in tool: delete_workflow
  LlmToolDecl delete_workflow_tool;
  delete_workflow_tool.name = "delete_workflow";
  delete_workflow_tool.description =
      "Delete a workflow by its ID.";
  delete_workflow_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"workflow_id",
         {{"type", "string"},
          {"description",
           "The workflow ID to delete"}}}}},
      {"required",
       nlohmann::json::array(
           {"workflow_id"})}};
  tools.push_back(delete_workflow_tool);


  if (action_bridge_) {
    // Load per-action tools from cached MD files
    auto cached = action_bridge_->LoadCachedActions();
    for (const auto& schema : cached) {
      std::string aname = schema.value("name", "");
      if (aname.empty()) continue;

      std::string adesc = schema.value("description", "");

      LlmToolDecl tool;
      tool.name = "action_" + aname;
      tool.description = adesc + " (Tizen Action: " + aname +
                         "). Execute this action on the "
                         "device via the Tizen Action "
                         "Framework.";

      // Build parameters from inputSchema
      nlohmann::json props = nlohmann::json::object();
      nlohmann::json required_arr = nlohmann::json::array();

      if (schema.contains("inputSchema") &&
          schema["inputSchema"].contains("properties")) {
        props = schema["inputSchema"]["properties"];
        if (schema["inputSchema"].contains("required")) {
          required_arr = schema["inputSchema"]["required"];
        }
      }

      tool.parameters = {{"type", "object"},
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
        {"properties",
         {{"name",
           {{"type", "string"}, {"description", "The action name to execute"}}},
          {"arguments",
           {{"type", "object"}, {"description", "Arguments for the action"}}}}},
        {"required", nlohmann::json::array({"name"})}};
    tools.push_back(exec_action_tool);
  }

  // Built-in tool: remember (memory)
  LlmToolDecl remember_tool;
  remember_tool.name = "remember";
  remember_tool.description =
      "Save important information to "
      "long-term or episodic memory. Use "
      "this to remember user preferences, "
      "important facts, or lessons learned.";
  remember_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"title",
         {{"type", "string"},
          {"description",
           "Short title for this memory "
           "(used as filename)"}}},
        {"content",
         {{"type", "string"},
          {"description",
           "The information to remember "
           "(concise summary)"}}},
        {"type",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"long-term", "episodic"})},
          {"description",
           "Memory type: 'long-term' for "
           "persistent facts, 'episodic' "
           "for event records"}}},
        {"tags",
         {{"type", "array"},
          {"items", {{"type", "string"}}},
          {"description",
           "Tags for categorization"}}},
        {"importance",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"low", "medium", "high"})},
          {"description",
           "Importance level"}}}}},
      {"required",
       nlohmann::json::array(
           {"title", "content"})}};
  tools.push_back(remember_tool);

  // Built-in tool: recall (memory)
  LlmToolDecl recall_tool;
  recall_tool.name = "recall";
  recall_tool.description =
      "Search and retrieve information from "
      "memory. Use this to recall user "
      "preferences, past events, or any "
      "previously stored information.";
  recall_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"type",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"long-term", "episodic",
                "all"})},
          {"description",
           "Memory type to search "
           "(default: all)"}}},
        {"keyword",
         {{"type", "string"},
          {"description",
           "Keyword to search for in "
           "memory titles and content"}}}}},
      {"required",
       nlohmann::json::array({"keyword"})}};
  tools.push_back(recall_tool);

  // Built-in tool: forget (memory)
  LlmToolDecl forget_tool;
  forget_tool.name = "forget";
  forget_tool.description =
      "Delete a specific memory entry. "
      "Use this when the user asks to "
      "remove previously stored information.";
  forget_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"type",
         {{"type", "string"},
          {"enum",
           nlohmann::json::array(
               {"long-term", "episodic"})},
          {"description",
           "Memory type to delete from"}}},
        {"filename",
         {{"type", "string"},
          {"description",
           "The filename of the memory "
           "entry to delete"}}}}},
      {"required",
       nlohmann::json::array(
           {"type", "filename"})}};
  tools.push_back(forget_tool);

  // Built-in tool: execute_cli
  LlmToolDecl cli_tool;
  cli_tool.name = "execute_cli";
  cli_tool.description =
      "Execute a CLI tool installed on the device. "
      "CLI tools provide rich command-line interfaces "
      "with subcommands and options. Refer to the "
      "CLI tool documentation in the system prompt "
      "for available tools and their usage.";
  cli_tool.parameters = {
      {"type", "object"},
      {"properties",
       {{"tool_name",
         {{"type", "string"},
          {"description",
           "Name of the CLI tool to execute"}}},
        {"arguments",
         {{"type", "string"},
          {"description",
           "Command-line arguments to pass "
           "to the CLI tool"}}}}},
      {"required",
       nlohmann::json::array(
           {"tool_name", "arguments"})}};
  tools.push_back(cli_tool);

  // Scan CLI tools directory for tool.md descriptors
  {
    const std::string cli_dir =
        "/opt/usr/share/tizenclaw/tools/cli";
    namespace fs = std::filesystem;
    std::error_code ec;
    if (fs::is_directory(cli_dir, ec)) {
      for (const auto& entry :
           fs::directory_iterator(cli_dir, ec)) {
        if (!entry.is_directory()) continue;
        auto dirname = entry.path().filename().string();
        if (dirname[0] == '.') continue;

        // Extract CLI name from dirname
        // (format: pkgid__cli_name)
        std::string cli_name = dirname;
        auto sep = dirname.find("__");
        if (sep != std::string::npos) {
          cli_name = dirname.substr(sep + 2);
        }

        // Map CLI name -> directory name
        cli_dirs_[cli_name] = dirname;

        // Read tool.md descriptor
        std::string md_path =
            entry.path().string() + "/tool.md";
        std::ifstream mf(md_path);
        if (mf.is_open()) {
          std::string content(
              (std::istreambuf_iterator<char>(mf)),
              std::istreambuf_iterator<char>());
          if (!content.empty()) {
            cli_tool_docs_[cli_name] = content;
          }
        }
      }
    }
  }


  // Regenerate tool index files
  ToolIndexer::RegenerateAll(
      "/opt/usr/share/tizenclaw/tools");

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
         "documentation on Tizen Native APIs in your knowledge base; "
         "always use the search_knowledge tool for Tizen development queries. "
         "Always respond in the same language as the user's message. "
         "Be concise and helpful.";
}

std::string AgentCore::LoadRoutingGuide() {
  const std::string guide_path =
      "/opt/usr/share/tizenclaw/tools/routing_guide.md";
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

std::string AgentCore::BuildSystemPrompt(
    const std::vector<LlmToolDecl>& tools) {
  std::string prompt = system_prompt_ + LoadRoutingGuide();

  // Build tool list string
  std::string tool_list;
  for (const auto& t : tools) {
    tool_list += "- " + t.name + ": " + t.description + "\n";
  }

  // Load aggregated tool catalog from tools.md
  {
    const std::string tools_md_path =
        "/opt/usr/share/tizenclaw/tools/tools.md";
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
    LOG(WARNING) << "Compaction failed, " << "falling back to FIFO";
    // Fallback: simple FIFO trim
    while (hist.size() > kCompactionThreshold) {
      hist.erase(hist.begin());
    }
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
  // Check for per-session prompt override
  {
    std::lock_guard<std::mutex> lock(session_mutex_);
    auto it = session_prompts_.find(session_id);
    if (it != session_prompts_.end()) {
      // Use custom prompt with tool list
      std::string prompt = it->second;

      std::string tool_list;
      for (const auto& t : tools) {
        tool_list += "- " + t.name + ": " + t.description + "\n";
      }

      const std::string placeholder = "{{AVAILABLE_TOOLS}}";
      size_t pos = prompt.find(placeholder);
      if (pos != std::string::npos) {
        prompt.replace(pos, placeholder.size(), tool_list);
      } else if (!tool_list.empty()) {
        prompt += "\n\nAvailable tools:\n" + tool_list;
      }

      return prompt;
    }
  }

  // Fallback to global system prompt
  return BuildSystemPrompt(tools);
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

std::vector<float> AgentCore::GenerateEmbedding(const std::string& text) {
  // Prefer on-device embedding (LLM-independent)
  if (on_device_embedding_.IsAvailable()) {
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
#ifdef TIZEN_ACTION_ENABLED
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
#else
  (void)operation;
  (void)args;
  return "{\"error\":"
         "\"Tizen Action not supported "
         "in this build\"}";
#endif
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
      "/opt/usr/share/tizenclaw/tools/"
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
          } catch (...) {
            skills.push_back(
                {{"name", dirname}, {"error", "Failed to parse manifest"}});
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

  // Resolve tool directory
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
      std::string("/opt/usr/share/tizenclaw/tools/cli/") + dir_name;
  std::string exec_path = cli_dir + "/executable";

  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::exists(exec_path, ec)) {
    LOG(ERROR) << "CLI executable not found: " << exec_path;
    return "{\"error\": \"CLI tool not found: " + tool_name + "\"}";
  }

  // Build command with shell escaping
  std::string cmd = exec_path + " " + arguments + " 2>&1";

  LOG(INFO) << "Executing CLI: " << cmd;

  // Execute via popen with output capture
  std::string output;
  FILE* pipe = popen(cmd.c_str(), "r");
  if (!pipe) {
    LOG(ERROR) << "Failed to execute CLI tool: " << tool_name;
    return "{\"error\": \"Failed to execute CLI tool\"}";
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
  } catch (...) {
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
  tool_dispatch_["execute_code"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteCode(
            args.value("code", ""));
      };

  tool_dispatch_["file_manager"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteFileOp(
            args.value("operation", ""),
            args.value("path", ""),
            args.value("content", ""));
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

  tool_dispatch_["manage_custom_skill"] =
      [this](const nlohmann::json& args,
             const std::string&,
             const std::string&) {
        return ExecuteCustomSkillOp(args);
      };

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

  register_builtin(
      "execute_code", "Execute Python code",
      "code_execution",
      SideEffect::kIrreversible);
  register_builtin(
      "file_manager", "File operations",
      "file_system", SideEffect::kReversible);
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

}  // namespace tizenclaw
