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
#ifndef PIPELINE_EXECUTOR_HH
#define PIPELINE_EXECUTOR_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

class AgentCore;  // forward declaration

// Pipeline step types
enum class PipelineStepType {
  kTool,       // Execute a built-in tool or skill
  kPrompt,     // Send prompt to LLM
  kCondition,  // if/then/else branch
};

// Single pipeline step definition
struct PipelineStep {
  std::string id;
  PipelineStepType type = PipelineStepType::kTool;
  std::string tool_name;  // For kTool
  nlohmann::json args;    // Tool args
  std::string prompt;     // For kPrompt

  // Conditional branching
  std::string condition;  // e.g. "{{x}} == ok"
  std::string then_step;  // Step ID on true
  std::string else_step;  // Step ID on false

  // Error handling
  bool skip_on_failure = false;
  int max_retries = 0;

  // Output variable name
  std::string output_var;
};

// Pipeline definition
struct Pipeline {
  std::string id;
  std::string name;
  std::string description;
  std::vector<PipelineStep> steps;
  std::string trigger;  // "manual" or "cron:..."
};

// Result of a pipeline execution
struct PipelineRunResult {
  std::string pipeline_id;
  std::string status;  // "success","failed","partial"
  std::map<std::string, nlohmann::json> variables;
  std::vector<std::pair<std::string, std::string>>
      step_results;  // step_id → result
  int duration_ms = 0;
};

class PipelineExecutor {
 public:
  explicit PipelineExecutor(AgentCore* agent);

  // Load pipelines from disk
  void LoadPipelines();

  // CRUD operations
  [[nodiscard]] std::string CreatePipeline(const nlohmann::json& def);
  [[nodiscard]] nlohmann::json ListPipelines() const;
  [[nodiscard]] bool DeletePipeline(const std::string& id);

  // Execute pipeline
  [[nodiscard]] PipelineRunResult RunPipeline(
      const std::string& pipeline_id, const nlohmann::json& input_vars = {});

  // Get pipeline by ID (for scheduler)
  [[nodiscard]] const Pipeline* GetPipeline(const std::string& id) const;

 private:
  // Variable interpolation in text
  std::string Interpolate(
      const std::string& tmpl,
      const std::map<std::string, nlohmann::json>& vars) const;

  // Variable interpolation in JSON
  nlohmann::json InterpolateJson(
      const nlohmann::json& j,
      const std::map<std::string, nlohmann::json>& vars) const;

  // Evaluate simple condition expression
  bool EvalCondition(const std::string& expr,
                     const std::map<std::string, nlohmann::json>& vars) const;

  // Execute a single step
  std::string ExecuteStep(const PipelineStep& step,
                          std::map<std::string, nlohmann::json>& vars,
                          const std::string& session_id);

  // Persistence
  void SavePipeline(const Pipeline& p) const;
  Pipeline LoadPipelineFile(const std::string& path) const;

  // Helpers
  static std::string GeneratePipelineId();
  std::string GetPipelinesDir() const;

  // Convert step type to/from string
  static std::string StepTypeToString(PipelineStepType type);
  static PipelineStepType StringToStepType(const std::string& s);

  AgentCore* agent_;
  std::map<std::string, Pipeline> pipelines_;
  mutable std::mutex pipelines_mutex_;

  static constexpr const char* kDataDir = "/opt/usr/share/tizenclaw";
};

}  // namespace tizenclaw

#endif  // PIPELINE_EXECUTOR_HH
