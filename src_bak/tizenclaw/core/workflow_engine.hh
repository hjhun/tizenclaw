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
#ifndef WORKFLOW_ENGINE_HH
#define WORKFLOW_ENGINE_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

class AgentCore;  // forward declaration

// Workflow step types
enum class WorkflowStepType {
  kPrompt,  // Send instruction to LLM
  kTool,    // Execute a tool directly
};

// Single workflow step definition
struct WorkflowStep {
  std::string id;           // e.g. "step_1"
  std::string description;  // Human-readable step title
  WorkflowStepType type = WorkflowStepType::kPrompt;
  std::string instruction;  // LLM prompt (for kPrompt)
  std::string tool_name;    // Tool to invoke (for kTool)
  nlohmann::json args;      // Tool arguments (for kTool)
  std::string output_var;   // Variable name for result
  bool skip_on_failure = false;
  int max_retries = 0;
};

// Workflow definition
struct Workflow {
  std::string id;
  std::string name;
  std::string description;
  std::string trigger;  // "manual" or "auto"
  std::vector<WorkflowStep> steps;
  std::string raw_markdown;  // Original Markdown text
};

// Result of a workflow execution
struct WorkflowRunResult {
  std::string workflow_id;
  std::string status;  // "success", "failed", "partial"
  std::vector<std::pair<std::string, std::string>>
      step_results;  // step_id → result
  std::map<std::string, nlohmann::json> variables;
  int duration_ms = 0;
};

class WorkflowEngine {
 public:
  explicit WorkflowEngine(AgentCore* agent);

  // Load workflows from disk
  void LoadWorkflows();

  // CRUD operations
  [[nodiscard]] std::string CreateWorkflow(
      const std::string& markdown);
  [[nodiscard]] nlohmann::json ListWorkflows() const;
  [[nodiscard]] bool DeleteWorkflow(const std::string& id);

  // Execute workflow
  [[nodiscard]] WorkflowRunResult RunWorkflow(
      const std::string& workflow_id,
      const nlohmann::json& input_vars = {});

  // Get workflow by ID
  [[nodiscard]] const Workflow* GetWorkflow(
      const std::string& id) const;

 private:
  // Parse Markdown string into Workflow struct
  [[nodiscard]] Workflow ParseMarkdown(
      const std::string& markdown) const;

  // Extract YAML frontmatter from markdown
  [[nodiscard]] nlohmann::json ParseFrontmatter(
      const std::string& yaml_block) const;

  // Parse individual step block
  [[nodiscard]] WorkflowStep ParseStepBlock(
      const std::string& id,
      const std::string& title,
      const std::string& body) const;

  // Variable interpolation: replace {{var}}
  [[nodiscard]] std::string Interpolate(
      const std::string& tmpl,
      const std::map<std::string, nlohmann::json>& vars)
      const;

  // Interpolate JSON values
  [[nodiscard]] nlohmann::json InterpolateJson(
      const nlohmann::json& j,
      const std::map<std::string, nlohmann::json>& vars)
      const;

  // Execute a single step
  [[nodiscard]] std::string ExecuteStep(
      const WorkflowStep& step,
      std::map<std::string, nlohmann::json>& vars,
      const std::string& session_id);

  // Persistence
  void SaveWorkflow(const Workflow& w) const;
  void RegenerateIndex() const;
  [[nodiscard]] Workflow LoadWorkflowFile(
      const std::string& path) const;

  // Helpers
  [[nodiscard]] static std::string GenerateWorkflowId();
  [[nodiscard]] std::string GetWorkflowsDir() const;

  // Convert step type to/from string
  [[nodiscard]] static std::string StepTypeToString(
      WorkflowStepType type);
  [[nodiscard]] static WorkflowStepType StringToStepType(
      const std::string& s);

  AgentCore* agent_;
  std::map<std::string, Workflow> workflows_;
  mutable std::mutex workflows_mutex_;

  static constexpr const char* kDataDir =
      "/opt/usr/share/tizenclaw";
};

}  // namespace tizenclaw

#endif  // WORKFLOW_ENGINE_HH
