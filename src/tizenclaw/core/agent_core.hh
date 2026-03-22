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
#ifndef AGENT_CORE_HH
#define AGENT_CORE_HH

#include <atomic>
#include <future>
#include <json.hpp>
#include <map>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

#include "../infra/container_engine.hh"
#include "../infra/health_monitor.hh"
#include "../llm/llm_backend.hh"
#include "../storage/session_store.hh"
#include "../storage/memory_store.hh"
#include "agent_role.hh"
#include "agent_factory.hh"
#include "tool_policy.hh"
#include "tool_router.hh"
#include "action_bridge.hh"
#include "auto_skill_agent.hh"
#include "system_context_provider.hh"
#include <functional>
#include <mutex>

#include "../scheduler/task_scheduler.hh"
#include "../storage/embedding_store.hh"
#include "../embedding/on_device_embedding.hh"
#include "pipeline_executor.hh"
#include "tool_dispatcher.hh"
#include "workflow_engine.hh"
#include "../channel/mcp_client_manager.hh"

namespace tizenclaw {

class AgentCore {
 public:
  AgentCore();
  ~AgentCore();

  [[nodiscard]] bool Initialize();
  void Shutdown();

  // Process a prompt, returns response text
  [[nodiscard]] std::string ProcessPrompt(
      const std::string& session_id, const std::string& prompt,
      std::function<void(const std::string&)> on_chunk = nullptr);

  // Clear a session from memory and disk
  void ClearSession(const std::string& session_id);

  // Direct skill execution for MCP (bypasses LLM,
  // but still uses container isolation)
  [[nodiscard]] std::string ExecuteSkillForMcp(const std::string& skill_name,
                                               const nlohmann::json& args);

  // Set TaskScheduler reference (called by daemon)
  void SetScheduler(TaskScheduler* scheduler) { scheduler_ = scheduler; }

  // Set HealthMonitor reference
  // (called by WebDashboard)
  void SetHealthMonitor(HealthMonitor* monitor) { health_monitor_ = monitor; }

  // Get SystemContextProvider reference
  SystemContextProvider* GetSystemContext() { return system_context_.get(); }

  // Access session store (for IPC usage queries)
  const SessionStore& GetSessionStore() const { return session_store_; }

  // Reload skill declarations (thread-safe)
  // Called by SkillWatcher on manifest changes
  void ReloadSkills();

  // Execute session management operations
  // (create_session, list_sessions,
  //  send_to_session)
  std::string ExecuteSessionOp(const std::string& operation,
                               const nlohmann::json& args,
                               const std::string& caller_session);

  // Execute supervisor operations
  // (run_supervisor, list_agent_roles)
  std::string ExecuteSupervisorOp(const std::string& operation,
                                  const nlohmann::json& args,
                                  const std::string& session_id);

  // Get tools filtered by allowed list
  // (empty list = all tools)
  [[nodiscard]] std::vector<LlmToolDecl> GetToolsFiltered(
      const std::vector<std::string>& allowed);

  // Execute pipeline operations
  // (create_pipeline, list_pipelines,
  //  run_pipeline, delete_pipeline)
  [[nodiscard]] std::string ExecutePipelineOp(const std::string& operation,
                                              const nlohmann::json& args,
                                              const std::string& session_id);

  // Execute workflow operations
  // (create_workflow, list_workflows,
  //  run_workflow, delete_workflow)
  [[nodiscard]] std::string ExecuteWorkflowOp(
      const std::string& operation,
      const nlohmann::json& args,
      const std::string& session_id);

  // Execute Tizen Action Framework operations
  // (list_actions, execute_action)
  [[nodiscard]] std::string ExecuteActionOp(const std::string& operation,
                                            const nlohmann::json& args);

  // Generate a structured tool.md document from
  // raw help output using LLM analysis.
  // Returns the generated markdown string.
  [[nodiscard]] std::string GenerateToolDoc(
      const std::string& tool_name,
      const std::string& binary_path,
      const std::string& help_output);

  // Connect to external MCP servers mapped in JSON config
  bool ConnectMcpServers(const std::string& config_path);

  // Return a JSON representation of connected MCP tools
  nlohmann::json GetMcpToolsJson();

  // Execute a tool via Bridge API (for WebApp).
  // Validates against the provided allowed_tools
  // list. Returns JSON result string.
  [[nodiscard]] std::string ExecuteBridgeTool(
      const std::string& tool_name,
      const nlohmann::json& args,
      const std::vector<std::string>&
          allowed_tools);

  // Get cached tool declarations (thread-safe)
  [[nodiscard]] std::vector<LlmToolDecl>
  GetToolDeclarations() const;

 private:
  // Execute a skill and return its JSON output
  std::string ExecuteSkill(const std::string& skill_name,
                           const nlohmann::json& args);

  // Execute arbitrary Python code (built-in tool)
  std::string ExecuteCode(const std::string& code);

  // Execute file operations (built-in tool)
  std::string ExecuteFileOp(const std::string& operation,
                            const std::string& path,
                            const std::string& content);

  // Execute task scheduler operations
  std::string ExecuteTaskOp(const std::string& operation,
                            const nlohmann::json& args);

  // Execute RAG operations
  // (ingest_document, search_knowledge)
  std::string ExecuteRagOp(const std::string& operation,
                           const nlohmann::json& args);

  // Lookup Tizen Web API reference docs
  // (list, read, search)
  std::string ExecuteWebApiLookup(const std::string& operation,
                                  const nlohmann::json& args);

  // Execute memory operations
  // (remember, recall, forget)
  std::string ExecuteMemoryOp(
      const std::string& operation,
      const nlohmann::json& args);

  // Execute custom skill management operations
  std::string ExecuteCustomSkillOp(const nlohmann::json& args);

  // Execute CLI tool operations
  std::string ExecuteCli(const std::string& tool_name,
                         const std::string& arguments);

  // Execute interactive/streaming CLI operations
  std::string ExecuteCliSessionOp(const std::string& operation,
                                  const nlohmann::json& args);

  // Generate a dynamic web app (built-in tool)
  std::string GenerateWebApp(const nlohmann::json& args);

  // Launch the Bridge WGT app with a web app URL
  bool LaunchBridgeApp(const std::string& app_id);


  // Generate embedding vector via LLM API
  std::vector<float> GenerateEmbedding(const std::string& text);

  // Load skill manifests as tool declarations
  std::vector<LlmToolDecl> LoadSkillDeclarations();

  // Load system prompt from config or file
  std::string LoadSystemPrompt(const nlohmann::json& config);

  // Load tool routing guide from MD
  std::string LoadRoutingGuide();

  // Load condensed web API catalog from
  // rag/web/index.md for system prompt injection
  std::string LoadWebApiCatalog();

  // Build final system prompt with dynamic
  // skill list
  std::string BuildSystemPrompt(const std::vector<LlmToolDecl>& tools);

  // Initialize tool dispatcher map
  void InitializeToolDispatcher();

  // Try fallback backends on primary failure
  LlmResponse TryFallbackBackends(
      const std::vector<LlmMessage>& history,
      const std::vector<LlmToolDecl>& tools,
      std::function<void(const std::string&)> on_chunk,
      const std::string& system_prompt);

  // Compact history via LLM summarization
  // MUST be called with session_mutex_ held
  void CompactHistory(const std::string& session_id);

  // Trim session history (compaction + FIFO)
  void TrimHistory(const std::string& session_id);

  std::unique_ptr<ContainerEngine> container_;
  std::shared_ptr<LlmBackend> backend_;
  std::mutex backend_mutex_;  // Protects backend_
  bool initialized_ = false;

  // Reload the active backend (e.g., when a new plugin is installed)
  void ReloadBackend();

  // Memory flush tracking
  std::atomic<int64_t> last_activity_time_{0};
  std::atomic<bool> stop_maintenance_{false};
  std::thread maintenance_thread_;
  void MaintenanceLoop();
  void UpdateActivityTime();

  // System prompt loaded from external file
  std::string system_prompt_;

  // Condensed web API catalog for system prompt
  std::string web_api_catalog_;

  // Session-based conversation history
  std::map<std::string, std::vector<LlmMessage>> sessions_;
  std::mutex session_mutex_;  // Protects sessions_

  // Per-session system prompt overrides
  // session_id → custom system_prompt
  std::map<std::string, std::string> session_prompts_;

  static constexpr size_t kMaxHistorySize = 30;
  static constexpr size_t kCompactionThreshold = 15;
  static constexpr size_t kCompactionCount = 10;

  SessionStore session_store_;
  ToolPolicy tool_policy_;
  ToolRouter tool_router_;

  // Cached skill declarations
  std::vector<LlmToolDecl> cached_tools_;
  std::atomic<bool> cached_tools_loaded_{false};
  std::mutex tools_mutex_;  // Protects cached_tools_

  // Skill runtime map: skill_name -> "python"|"node"|"native"
  std::map<std::string, std::string> skill_runtimes_;

  // Skill directory map: manifest_name -> actual_dirname
  // (e.g. "get_sample_info" -> "org.tizen....__get_sample_info")
  std::map<std::string, std::string> skill_dirs_;

  // CLI tool directory map: cli_name -> actual_dirname
  std::map<std::string, std::string> cli_dirs_;

  // CLI tool documentation cache: cli_name -> tool.md content
  std::map<std::string, std::string> cli_tool_docs_;

  // Model fallback configuration
  std::vector<std::string> fallback_names_;
  nlohmann::json llm_config_;

  // Embedding store for RAG
  EmbeddingStore embedding_store_;

  // Safe lock for lazy embedding
  mutable std::mutex embedding_mutex_;
  OnDeviceEmbedding on_device_embedding_;

  // Memory store for persistent memory
  MemoryStore memory_store_;

  // Tool dispatch map (name -> handler)
  //   args, tool_name, session_id
  using ToolHandler = std::function<std::string(
      const nlohmann::json&, const std::string&,
      const std::string&)>;
  std::unordered_map<std::string, ToolHandler>
      tool_dispatch_;

  // Modular tool dispatcher (TODO-07)
  std::unique_ptr<ToolDispatcher> tool_dispatcher_;

  // Task scheduler (owned by daemon)
  TaskScheduler* scheduler_ = nullptr;

  // Health monitor (owned by WebDashboard)
  HealthMonitor* health_monitor_ = nullptr;

  // Supervisor engine for multi-agent
  std::unique_ptr<SupervisorEngine> supervisor_;

  // Pipeline executor for workflows
  std::unique_ptr<PipelineExecutor> pipeline_executor_;

  // Workflow engine for MD-based workflows
  std::unique_ptr<WorkflowEngine> workflow_engine_;

  // Tizen Action Framework bridge
  std::unique_ptr<ActionBridge> action_bridge_;

  // System context provider (EventBus subscriber)
  std::unique_ptr<SystemContextProvider> system_context_;

  // Agent factory for dynamic agent creation
  std::unique_ptr<AgentFactory> agent_factory_;

  // Auto skill generation agent
  std::unique_ptr<AutoSkillAgent> auto_skill_agent_;

  // Manager for connecting to external MCP Servers
  std::unique_ptr<McpClientManager> mcp_client_manager_;

  // Get session-specific system prompt
  // (falls back to global system_prompt_)
  std::string GetSessionPrompt(const std::string& session_id,
                               const std::vector<LlmToolDecl>& tools);
  // SwitchToBestBackend: Unified algorithm to select and initialize the
  // best LLM backend based on priority configurations. Active backend is
  // IntMax, plugins and fallbacks have configurable priority, descending sort.
  bool SwitchToBestBackend(bool is_reload = false);
};

}  // namespace tizenclaw

#endif  // AGENT_CORE_HH
