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
#include "tool_declaration_builder.hh"

#include <filesystem>
#include <fstream>

#include "../../common/logging.hh"

namespace tizenclaw {

void ToolDeclarationBuilder::AppendBuiltinTools(
    std::vector<LlmToolDecl>& tools) {
#ifdef TIZEN_FEATURE_CODE_GENERATOR
  // execute_code
  {
    LlmToolDecl t;
    t.name = "execute_code";
    t.description =
        "Execute arbitrary Python code on the Tizen "
        "device. Use this when no existing skill/tool "
        "can accomplish the task. The code MUST print "
        "a JSON result to stdout as the last line. "
        "Available: ctypes for Tizen C-API, os, "
        "subprocess, json, sys. "
        "Libraries at /tizen_libs or system path.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"code",
           {{"type", "string"},
            {"description",
             "Python code to execute on the "
             "Tizen device"}}}}},
        {"required",
         nlohmann::json::array({"code"})}};
    tools.push_back(t);
  }
#endif  // TIZEN_FEATURE_CODE_GENERATOR

  // file_manager removed — use tizen-file-manager-cli
  // via execute_cli instead

  // create_task
  {
    LlmToolDecl t;
    t.name = "create_task";
    t.description =
        "Create a scheduled task that runs "
        "automatically. Supports: "
        "'daily HH:MM' (every day), "
        "'interval Ns/Nm/Nh' (repeating), "
        "'once YYYY-MM-DD HH:MM' (one-shot), "
        "'weekly DAY HH:MM' (every week). "
        "The prompt will be sent to the LLM "
        "at the scheduled time.";
    t.parameters = {
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
        {"required",
         nlohmann::json::array(
             {"schedule", "prompt"})}};
    tools.push_back(t);
  }

  // list_tasks
  {
    LlmToolDecl t;
    t.name = "list_tasks";
    t.description =
        "List all scheduled tasks. Optionally "
        "filter by session_id.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"session_id",
           {{"type", "string"},
            {"description",
             "Optional session ID to "
             "filter"}}}}},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // cancel_task
  {
    LlmToolDecl t;
    t.name = "cancel_task";
    t.description =
        "Cancel a scheduled task by its ID.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"task_id",
           {{"type", "string"},
            {"description",
             "The task ID to cancel"}}}}},
        {"required",
         nlohmann::json::array({"task_id"})}};
    tools.push_back(t);
  }

  // create_session
  {
    LlmToolDecl t;
    t.name = "create_session";
    t.description =
        "Create a new agent session with a custom "
        "system prompt. The new session operates "
        "independently with its own conversation "
        "history. Use this to delegate specialized "
        "tasks to a purpose-built agent.";
    t.parameters = {
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
        {"required",
         nlohmann::json::array(
             {"name", "system_prompt"})}};
    tools.push_back(t);
  }

  // list_sessions
  {
    LlmToolDecl t;
    t.name = "list_sessions";
    t.description =
        "List all active agent sessions with "
        "their names and system prompts.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // send_to_session
  {
    LlmToolDecl t;
    t.name = "send_to_session";
    t.description =
        "Send a message to another agent session "
        "and receive its response. The target "
        "session processes the message using its "
        "own system prompt and conversation "
        "history. Use this for inter-agent "
        "communication and task delegation.";
    t.parameters = {
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
        {"required",
         nlohmann::json::array(
             {"target_session", "message"})}};
    tools.push_back(t);
  }

#ifdef TIZEN_FEATURE_CODE_GENERATOR
  // manage_custom_skill
  {
    LlmToolDecl t;
    t.name = "manage_custom_skill";
    t.description =
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
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"operation",
           {{"type", "string"},
            {"enum",
             nlohmann::json::array(
                 {"create", "update",
                  "delete", "list"})},
            {"description",
             "Operation to perform"}}},
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
                 {"python", "node",
                  "native"})},
            {"description",
             "Skill runtime "
             "(default: python)"}}},
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
            {"enum",
             nlohmann::json::array(
                 {"low", "medium", "high"})},
            {"description",
             "Risk level (default: low)"}}},
          {"category",
           {{"type", "string"},
            {"description",
             "Category of the skill "
             "(e.g. App Management, "
             "Device Info, Network)"}}}}},
        {"required",
         nlohmann::json::array(
             {"operation"})}};
    tools.push_back(t);
  }
#endif  // TIZEN_FEATURE_CODE_GENERATOR

  // ingest_document (RAG)
  {
    LlmToolDecl t;
    t.name = "ingest_document";
    t.description =
        "Ingest a document into the knowledge "
        "base for semantic search. The text is "
        "split into chunks, embedded, and "
        "stored in the local vector database.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"source",
           {{"type", "string"},
            {"description",
             "Source identifier (filename, "
             "URL, or label)"}}},
          {"text",
           {{"type", "string"},
            {"description",
             "The document text to "
             "ingest"}}}}},
        {"required",
         nlohmann::json::array(
             {"source", "text"})}};
    tools.push_back(t);
  }

  // search_knowledge (RAG)
  {
    LlmToolDecl t;
    t.name = "search_knowledge";
    t.description =
        "Search the knowledge base using "
        "semantic similarity. Returns the "
        "most relevant document chunks.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"query",
           {{"type", "string"},
            {"description",
             "The search query"}}},
          {"top_k",
           {{"type", "integer"},
            {"description",
             "Number of results "
             "(default 5)"}}}}},
        {"required",
         nlohmann::json::array({"query"})}};
    tools.push_back(t);
  }

  // lookup_web_api (Tizen Web API reference)
  {
    LlmToolDecl t;
    t.name = "lookup_web_api";
    t.description =
        "Look up Tizen Web API reference documentation. "
        "Use 'list' to see all available API guides and "
        "Doxygen references (returns index.md). "
        "Use 'read' with a path from the index to read "
        "a specific document. "
        "Use 'search' with a query to find matching "
        "documents by keyword. "
        "ALWAYS use this tool when generating Tizen "
        "web app code that uses device APIs.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"operation",
           {{"type", "string"},
            {"enum",
             nlohmann::json::array(
                 {"list", "read", "search"})},
            {"description",
             "list: show index, "
             "read: get a doc by path, "
             "search: keyword search"}}},
          {"path",
           {{"type", "string"},
            {"description",
             "Relative path within the "
             "web API docs (for 'read'). "
             "e.g. 'guides/alarm/alarms.md' "
             "or 'api/10.0/device_api/"
             "mobile/tizen/alarm.md'"}}},
          {"query",
           {{"type", "string"},
            {"description",
             "Search keyword "
             "(for 'search')"}}}}},
        {"required",
         nlohmann::json::array(
             {"operation"})}};
    tools.push_back(t);
  }

  // run_supervisor
  {
    LlmToolDecl t;
    t.name = "run_supervisor";
    t.description =
        "Run a supervisor agent that decomposes "
        "a complex goal into sub-tasks and "
        "delegates them to specialized role "
        "agents. Each role agent has its own "
        "system prompt and tool restrictions. "
        "Results are aggregated into a single "
        "response. Requires agent_roles.json "
        "configuration.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"goal",
           {{"type", "string"},
            {"description",
             "The high-level goal to "
             "decompose and delegate"}}},
          {"strategy",
           {{"type", "string"},
            {"enum",
             nlohmann::json::array(
                 {"sequential", "parallel"})},
            {"description",
             "Execution strategy: "
             "'sequential' (default) or "
             "'parallel'"}}}}},
        {"required",
         nlohmann::json::array({"goal"})}};
    tools.push_back(t);
  }

  // list_agent_roles
  {
    LlmToolDecl t;
    t.name = "list_agent_roles";
    t.description =
        "List all configured agent roles with "
        "their names, system prompts, and "
        "allowed tools.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // get_agent_status
  {
    LlmToolDecl t;
    t.name = "get_agent_status";
    t.description =
        "Get current agent system status: "
        "configured agents count, active "
        "delegations in progress, and recent "
        "delegation history with stats.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // list_agents
  {
    LlmToolDecl t;
    t.name = "list_agents";
    t.description =
        "List all running agents with their "
        "status. Returns configured roles, "
        "dynamically created agents, active "
        "delegations, event bus sources, and "
        "autonomous trigger status.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // spawn_agent
  {
    LlmToolDecl t;
    t.name = "spawn_agent";
    t.description =
        "Create a new specialized agent with a "
        "custom role definition. The agent is "
        "dynamically registered and can be "
        "delegated tasks via run_supervisor. "
        "Use this when existing agents are "
        "insufficient for a new task domain.";
    t.parameters = {
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
    tools.push_back(t);
  }

  // list_dynamic_agents
  {
    LlmToolDecl t;
    t.name = "list_dynamic_agents";
    t.description =
        "List all dynamically created agents "
        "that were spawned at runtime.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // remove_agent
  {
    LlmToolDecl t;
    t.name = "remove_agent";
    t.description =
        "Remove a dynamically created agent.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"name",
           {{"type", "string"},
            {"description",
             "Name of the dynamic agent "
             "to remove"}}}}},
        {"required",
         nlohmann::json::array({"name"})}};
    tools.push_back(t);
  }

  // create_pipeline
  {
    LlmToolDecl t;
    t.name = "create_pipeline";
    t.description =
        "Create a multi-step pipeline for "
        "deterministic workflow execution. "
        "Each step can be a tool call, LLM "
        "prompt, or conditional branch. "
        "Steps execute sequentially, and "
        "output from each step is available "
        "to subsequent steps via "
        "{{variable}} interpolation.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"name",
           {{"type", "string"},
            {"description",
             "Pipeline name"}}},
          {"description",
           {{"type", "string"},
            {"description",
             "Pipeline description"}}},
          {"trigger",
           {{"type", "string"},
            {"description",
             "Trigger type: 'manual' or "
             "'cron:daily HH:MM' etc."}}},
          {"steps",
           {{"type", "array"},
            {"description",
             "Array of step objects"},
            {"items",
             {{"type", "object"},
              {"properties",
               {{"id",
                 {{"type", "string"},
                  {"description",
                   "Step identifier"}}},
                {"type",
                 {{"type", "string"},
                  {"description",
                   "Step type: tool, "
                   "prompt, or condition"}}},
                {"tool_name",
                 {{"type", "string"},
                  {"description",
                   "Tool to invoke"}}},
                {"args",
                 {{"type", "object"},
                  {"description",
                   "Tool arguments"}}},
                {"prompt",
                 {{"type", "string"},
                  {"description",
                   "LLM prompt text"}}},
                {"condition",
                 {{"type", "string"},
                  {"description",
                   "Condition expression"}}},
                {"then_step",
                 {{"type", "string"},
                  {"description",
                   "Step ID if true"}}},
                {"else_step",
                 {{"type", "string"},
                  {"description",
                   "Step ID if false"}}},
                {"output_var",
                 {{"type", "string"},
                  {"description",
                   "Variable name for "
                   "step output"}}},
                {"skip_on_failure",
                 {{"type", "boolean"},
                  {"description",
                   "Continue on error"}}},
                {"max_retries",
                 {{"type", "integer"},
                  {"description",
                   "Max retry count"}}}}},
              {"required",
               nlohmann::json::array(
                   {"id", "type"})}}}}}}},
        {"required",
         nlohmann::json::array(
             {"name", "steps"})}};
    tools.push_back(t);
  }

  // list_pipelines
  {
    LlmToolDecl t;
    t.name = "list_pipelines";
    t.description =
        "List all configured pipelines with "
        "their names, triggers, and step "
        "counts.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // run_pipeline
  {
    LlmToolDecl t;
    t.name = "run_pipeline";
    t.description =
        "Execute a pipeline by its ID. "
        "Optionally provide input variables "
        "that can be referenced in steps "
        "via {{variable}} syntax.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"pipeline_id",
           {{"type", "string"},
            {"description",
             "The pipeline ID to "
             "execute"}}},
          {"input_vars",
           {{"type", "object"},
            {"description",
             "Input variables (key-value "
             "pairs) available to all "
             "pipeline steps"}}}}},
        {"required",
         nlohmann::json::array(
             {"pipeline_id"})}};
    tools.push_back(t);
  }

  // delete_pipeline
  {
    LlmToolDecl t;
    t.name = "delete_pipeline";
    t.description =
        "Delete a pipeline by its ID.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"pipeline_id",
           {{"type", "string"},
            {"description",
             "The pipeline ID to "
             "delete"}}}}},
        {"required",
         nlohmann::json::array(
             {"pipeline_id"})}};
    tools.push_back(t);
  }

  // create_workflow
  {
    LlmToolDecl t;
    t.name = "create_workflow";
    t.description =
        "Create a workflow from Markdown text. "
        "The markdown must include YAML "
        "frontmatter (---) with 'name' field "
        "and '## Step N:' sections with "
        "type/instruction/tool_name/output_var "
        "metadata. Steps execute sequentially "
        "with {{variable}} interpolation.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"markdown",
           {{"type", "string"},
            {"description",
             "Markdown text with YAML "
             "frontmatter and Step "
             "sections"}}}}},
        {"required",
         nlohmann::json::array(
             {"markdown"})}};
    tools.push_back(t);
  }

  // list_workflows
  {
    LlmToolDecl t;
    t.name = "list_workflows";
    t.description =
        "List all registered workflows with "
        "their names, descriptions, triggers, "
        "and step counts.";
    t.parameters = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
    tools.push_back(t);
  }

  // run_workflow
  {
    LlmToolDecl t;
    t.name = "run_workflow";
    t.description =
        "Execute a workflow by its ID. "
        "Optionally provide input variables "
        "that can be referenced in steps "
        "via {{variable}} syntax.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"workflow_id",
           {{"type", "string"},
            {"description",
             "The workflow ID to "
             "execute"}}},
          {"input_vars",
           {{"type", "object"},
            {"description",
             "Input variables (key-value "
             "pairs) for steps"}}}}},
        {"required",
         nlohmann::json::array(
             {"workflow_id"})}};
    tools.push_back(t);
  }

  // delete_workflow
  {
    LlmToolDecl t;
    t.name = "delete_workflow";
    t.description =
        "Delete a workflow by its ID.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"workflow_id",
           {{"type", "string"},
            {"description",
             "The workflow ID to "
             "delete"}}}}},
        {"required",
         nlohmann::json::array(
             {"workflow_id"})}};
    tools.push_back(t);
  }

  // remember (memory)
  {
    LlmToolDecl t;
    t.name = "remember";
    t.description =
        "Save important information to "
        "long-term or episodic memory. Use "
        "this to remember user preferences, "
        "important facts, or lessons learned.";
    t.parameters = {
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
    tools.push_back(t);
  }

  // recall (memory)
  {
    LlmToolDecl t;
    t.name = "recall";
    t.description =
        "Search and retrieve information from "
        "memory. Use this to recall user "
        "preferences, past events, or any "
        "previously stored information.";
    t.parameters = {
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
             "memory titles and "
             "content"}}}}},
        {"required",
         nlohmann::json::array(
             {"keyword"})}};
    tools.push_back(t);
  }

  // forget (memory)
  {
    LlmToolDecl t;
    t.name = "forget";
    t.description =
        "Delete a specific memory entry. "
        "Use this when the user asks to "
        "remove previously stored information.";
    t.parameters = {
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
    tools.push_back(t);
  }

  // execute_cli
  {
    LlmToolDecl t;
    t.name = "execute_cli";
    t.description =
        "Execute a CLI tool installed on the "
        "device. CLI tools provide rich "
        "command-line interfaces with "
        "subcommands and options. Refer to the "
        "CLI tool documentation in the system "
        "prompt for available tools and their "
        "usage.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"tool_name",
           {{"type", "string"},
            {"description",
             "Name of the CLI tool to "
             "execute"}}},
          {"arguments",
           {{"type", "string"},
            {"description",
             "Command-line arguments to "
             "pass to the CLI tool"}}}}},
        {"required",
         nlohmann::json::array(
             {"tool_name", "arguments"})}};
    tools.push_back(t);
  }

  // start_cli_session
  {
    LlmToolDecl t;
    t.name = "start_cli_session";
    t.description =
        "Start an interactive or streaming CLI tool session. "
        "Use this for tools that require continuous input/output "
        "or event monitoring (e.g., vconf watch). Returns a session_id.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"tool_name",
           {{"type", "string"},
            {"description", "Name of the CLI tool"}}},
          {"arguments",
           {{"type", "string"},
            {"description", "Command-line arguments"}}},
          {"mode",
           {{"type", "string"},
            {"enum", {"interactive", "streaming", "pipe"}},
            {"description", "Execution mode"}}},
          {"timeout",
           {{"type", "integer"},
            {"description", "Session timeout in seconds (default: 60)"}}}}},
        {"required", nlohmann::json::array({"tool_name", "arguments"})}};
    tools.push_back(t);
  }

  // send_to_cli
  {
    LlmToolDecl t;
    t.name = "send_to_cli";
    t.description = "Send interactive input to a running CLI session.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"session_id",
           {{"type", "string"}, {"description", "ID of the active session"}}},
          {"input",
           {{"type", "string"}, {"description", "Input string to send"}}},
          {"read_timeout_ms",
           {{"type", "integer"},
            {"description", "Milliseconds to wait for output (default: 2000)"}}}}},
        {"required", nlohmann::json::array({"session_id", "input"})}};
    tools.push_back(t);
  }

  // read_cli_output
  {
    LlmToolDecl t;
    t.name = "read_cli_output";
    t.description = "Read continuous or pending output from a CLI session.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"session_id",
           {{"type", "string"}, {"description", "ID of the active session"}}},
          {"read_timeout_ms",
           {{"type", "integer"},
            {"description", "Milliseconds to wait for output (default: 1000)"}}}}},
        {"required", nlohmann::json::array({"session_id"})}};
    tools.push_back(t);
  }

  // close_cli_session
  {
    LlmToolDecl t;
    t.name = "close_cli_session";
    t.description = "Terminate a running CLI session.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"session_id",
           {{"type", "string"},
            {"description", "ID of the session to terminate"}}}}},
        {"required", nlohmann::json::array({"session_id"})}};
    tools.push_back(t);
  }

  // generate_web_app
  {
    LlmToolDecl t;
    t.name = "generate_web_app";
    t.description =
        "Generate or update a web application "
        "that runs in the TizenClaw Bridge (WRT). "
        "The app has access to Tizen Web Device "
        "APIs (tizen.systeminfo, tizen.application, "
        "tizen.alarm, tizen.notification, "
        "tizen.power, tizen.sound, tizen.bluetooth, "
        "etc.) since it runs in the WRT context. "
        "Accessible at "
        "http://<device-ip>:9090/apps/<app_id>/. "
        "To UPDATE an existing app, use the same "
        "app_id — files will be overwritten. "
        "To modify specific files of an existing "
        "app, use tizen-file-manager-cli via "
        "execute_cli tool.";
    t.parameters = {
        {"type", "object"},
        {"properties",
         {{"app_id",
           {{"type", "string"},
            {"description",
             "Unique identifier for the app "
             "(lowercase alphanumeric + "
             "underscore, max 64 chars)"}}},
          {"title",
           {{"type", "string"},
            {"description",
             "Display title for the web app"}}},
          {"html",
           {{"type", "string"},
            {"description",
             "Complete HTML content. Can be a "
             "single-file app with inline "
             "CSS/JS, or just the HTML "
             "structure referencing "
             "style.css and app.js"}}},
          {"css",
           {{"type", "string"},
            {"description",
             "Optional separate CSS stylesheet "
             "(saved as style.css)"}}},
          {"js",
           {{"type", "string"},
            {"description",
             "Optional separate JavaScript "
             "code (saved as app.js)"}}},
          {"assets",
           {{"type", "array"},
            {"description",
             "Optional array of external assets "
             "to download (images, fonts, etc). "
             "Each item: {\"url\": \"...\", "
             "\"filename\": \"...\"}. Max 10MB "
             "per file."},
            {"items",
             {{"type", "object"},
              {"properties",
               {{"url",
                 {{"type", "string"},
                  {"description",
                   "URL to download"}}},
                {"filename",
                 {{"type", "string"},
                  {"description",
                   "Local filename to save "
                   "as (e.g. logo.png)"}}}}}}}}},
          {"allowed_tools",
           {{"type", "array"},
            {"items", {{"type", "string"}}},
            {"description",
             "Optional list of tool names "
             "this app can call via the "
             "Bridge API (e.g. "
             "[\"get_battery_info\", "
             "\"control_display\"]). If "
             "omitted, no tools are "
             "accessible."}}}}},
        {"required",
         nlohmann::json::array(
             {"app_id", "title", "html"})}};
    tools.push_back(t);
  }
}

void ToolDeclarationBuilder::AppendActionTools(
    std::vector<LlmToolDecl>& tools,
    ActionBridge* action_bridge) {
  if (!action_bridge) return;

  // Load per-action tools from cached MD files
  auto cached = action_bridge->LoadCachedActions();
  for (const auto& schema : cached) {
    std::string aname =
        schema.value("name", "");
    if (aname.empty()) continue;

    std::string adesc =
        schema.value("description", "");

    LlmToolDecl tool;
    tool.name = "action_" + aname;
    tool.description =
        adesc + " (Tizen Action: " + aname +
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
      {"properties",
       {{"name",
         {{"type", "string"},
          {"description",
           "The action name to execute"}}},
        {"arguments",
         {{"type", "object"},
          {"description",
           "Arguments for the action"}}}}},
      {"required",
       nlohmann::json::array({"name"})}};
  tools.push_back(exec_action_tool);
}

void ToolDeclarationBuilder::AppendCliTools(
    std::vector<LlmToolDecl>& tools,
    std::map<std::string, std::string>& cli_dirs,
    std::map<std::string, std::string>&
        cli_docs) {
  const std::string cli_dir =
      "/opt/usr/share/tizenclaw/tools/cli";
  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::is_directory(cli_dir, ec)) return;

  for (const auto& entry :
       fs::directory_iterator(cli_dir, ec)) {
    if (!entry.is_directory()) continue;
    auto dirname =
        entry.path().filename().string();
    if (dirname[0] == '.') continue;

    // Extract CLI name from dirname
    // (format: pkgid__cli_name)
    std::string cli_name = dirname;
    auto sep = dirname.find("__");
    if (sep != std::string::npos) {
      cli_name = dirname.substr(sep + 2);
    }

    // Map CLI name -> directory name
    cli_dirs[cli_name] = dirname;

    // Read tool.md descriptor
    std::string md_path =
        entry.path().string() + "/tool.md";
    std::ifstream mf(md_path);
    if (mf.is_open()) {
      std::string content(
          (std::istreambuf_iterator<char>(mf)),
          std::istreambuf_iterator<char>());
      if (!content.empty()) {
        cli_docs[cli_name] = content;
      }
    }
  }
}

}  // namespace tizenclaw
