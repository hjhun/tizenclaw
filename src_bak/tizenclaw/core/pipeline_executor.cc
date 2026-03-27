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
#include "pipeline_executor.hh"

#include <dirent.h>
#include <sys/stat.h>

#include <atomic>
#include <chrono>
#include <fstream>
#include <regex>
#include <sstream>

#include "../../common/logging.hh"
#include "../storage/audit_logger.hh"
#include "agent_core.hh"

namespace tizenclaw {

PipelineExecutor::PipelineExecutor(AgentCore* agent) : agent_(agent) {}

std::string PipelineExecutor::GetPipelinesDir() const {
  return std::string(kDataDir) + "/pipelines";
}

std::string PipelineExecutor::GeneratePipelineId() {
  static std::atomic<int> counter{0};
  auto now = std::chrono::system_clock::now();
  auto ts = std::chrono::duration_cast<std::chrono::milliseconds>(
                now.time_since_epoch())
                .count();
  int seq = counter.fetch_add(1);
  std::ostringstream oss;
  oss << "pipe-" << std::hex << ts << "-" << seq;
  return oss.str();
}

std::string PipelineExecutor::StepTypeToString(PipelineStepType type) {
  switch (type) {
    case PipelineStepType::kTool:
      return "tool";
    case PipelineStepType::kPrompt:
      return "prompt";
    case PipelineStepType::kCondition:
      return "condition";
    default:
      return "tool";
  }
}

PipelineStepType PipelineExecutor::StringToStepType(const std::string& s) {
  if (s == "prompt") return PipelineStepType::kPrompt;
  if (s == "condition") return PipelineStepType::kCondition;
  return PipelineStepType::kTool;
}

void PipelineExecutor::LoadPipelines() {
  std::string dir = GetPipelinesDir();
  DIR* d = opendir(dir.c_str());
  if (!d) {
    mkdir(dir.c_str(), 0755);
    return;
  }

  std::lock_guard<std::mutex> lock(pipelines_mutex_);
  pipelines_.clear();

  struct dirent* ent;
  while ((ent = readdir(d)) != nullptr) {
    std::string fname = ent->d_name;
    if (fname.size() < 6 || fname.substr(fname.size() - 5) != ".json") {
      continue;
    }

    std::string path = dir + "/" + fname;
    try {
      Pipeline p = LoadPipelineFile(path);
      if (!p.id.empty()) {
        pipelines_[p.id] = std::move(p);
      }
    } catch (const std::exception& e) {
      LOG(WARNING) << "Failed to load pipeline: " << path << ": " << e.what();
    }
  }
  closedir(d);

  LOG(INFO) << "Loaded " << pipelines_.size() << " pipelines";
}

Pipeline PipelineExecutor::LoadPipelineFile(const std::string& path) const {
  std::ifstream f(path);
  if (!f.is_open()) {
    throw std::runtime_error("Cannot open: " + path);
  }

  nlohmann::json j;
  f >> j;
  f.close();

  Pipeline p;
  p.id = j.value("id", "");
  p.name = j.value("name", "");
  p.description = j.value("description", "");
  p.trigger = j.value("trigger", "manual");

  if (j.contains("steps") && j["steps"].is_array()) {
    for (auto& sj : j["steps"]) {
      PipelineStep step;
      step.id = sj.value("id", "");
      step.type = StringToStepType(sj.value("type", "tool"));
      step.tool_name = sj.value("tool_name", "");
      step.prompt = sj.value("prompt", "");
      step.condition = sj.value("condition", "");
      step.then_step = sj.value("then_step", "");
      step.else_step = sj.value("else_step", "");
      step.skip_on_failure = sj.value("skip_on_failure", false);
      step.max_retries = sj.value("max_retries", 0);
      step.output_var = sj.value("output_var", "");
      if (sj.contains("args")) {
        step.args = sj["args"];
      }
      p.steps.push_back(step);
    }
  }

  return p;
}

void PipelineExecutor::SavePipeline(const Pipeline& p) const {
  std::string dir = GetPipelinesDir();
  mkdir(dir.c_str(), 0755);

  nlohmann::json j;
  j["id"] = p.id;
  j["name"] = p.name;
  j["description"] = p.description;
  j["trigger"] = p.trigger;

  nlohmann::json steps = nlohmann::json::array();
  for (auto& step : p.steps) {
    nlohmann::json sj;
    sj["id"] = step.id;
    sj["type"] = StepTypeToString(step.type);
    if (!step.tool_name.empty()) sj["tool_name"] = step.tool_name;
    if (!step.args.is_null() && !step.args.empty()) sj["args"] = step.args;
    if (!step.prompt.empty()) sj["prompt"] = step.prompt;
    if (!step.condition.empty()) sj["condition"] = step.condition;
    if (!step.then_step.empty()) sj["then_step"] = step.then_step;
    if (!step.else_step.empty()) sj["else_step"] = step.else_step;
    if (step.skip_on_failure) sj["skip_on_failure"] = true;
    if (step.max_retries > 0) sj["max_retries"] = step.max_retries;
    if (!step.output_var.empty()) sj["output_var"] = step.output_var;
    steps.push_back(sj);
  }
  j["steps"] = steps;

  std::string path = dir + "/" + p.id + ".json";

  // Atomic write via temp file
  std::string tmp = path + ".tmp";
  std::ofstream f(tmp);
  if (f.is_open()) {
    f << j.dump(2);
    f.close();
    rename(tmp.c_str(), path.c_str());
  }
}

std::string PipelineExecutor::CreatePipeline(const nlohmann::json& def) {
  Pipeline p;
  p.id = GeneratePipelineId();
  p.name = def.value("name", "");
  p.description = def.value("description", "");
  p.trigger = def.value("trigger", "manual");

  if (p.name.empty()) {
    return "";  // Name required
  }

  if (def.contains("steps") && def["steps"].is_array()) {
    int step_idx = 0;
    for (auto& sj : def["steps"]) {
      PipelineStep step;
      step.id = sj.value("id", "step_" + std::to_string(step_idx));
      step.type = StringToStepType(sj.value("type", "tool"));
      step.tool_name = sj.value("tool_name", "");
      step.prompt = sj.value("prompt", "");
      step.condition = sj.value("condition", "");
      step.then_step = sj.value("then_step", "");
      step.else_step = sj.value("else_step", "");
      step.skip_on_failure = sj.value("skip_on_failure", false);
      step.max_retries = sj.value("max_retries", 0);
      step.output_var = sj.value("output_var", step.id);
      if (sj.contains("args")) {
        step.args = sj["args"];
      }
      p.steps.push_back(step);
      step_idx++;
    }
  }

  if (p.steps.empty()) {
    return "";  // At least one step required
  }

  {
    std::lock_guard<std::mutex> lock(pipelines_mutex_);
    pipelines_[p.id] = p;
  }

  SavePipeline(p);
  LOG(INFO) << "Created pipeline: " << p.id << " (" << p.name << ")";

  return p.id;
}

nlohmann::json PipelineExecutor::ListPipelines() const {
  std::lock_guard<std::mutex> lock(pipelines_mutex_);
  nlohmann::json result = nlohmann::json::array();

  for (auto& [id, p] : pipelines_) {
    result.push_back({{"id", id},
                      {"name", p.name},
                      {"description", p.description},
                      {"trigger", p.trigger},
                      {"steps_count", static_cast<int>(p.steps.size())}});
  }

  return result;
}

bool PipelineExecutor::DeletePipeline(const std::string& id) {
  std::lock_guard<std::mutex> lock(pipelines_mutex_);
  auto it = pipelines_.find(id);
  if (it == pipelines_.end()) {
    return false;
  }

  // Delete file
  std::string path = GetPipelinesDir() + "/" + id + ".json";
  remove(path.c_str());

  pipelines_.erase(it);
  LOG(INFO) << "Deleted pipeline: " << id;
  return true;
}

const Pipeline* PipelineExecutor::GetPipeline(const std::string& id) const {
  std::lock_guard<std::mutex> lock(pipelines_mutex_);
  auto it = pipelines_.find(id);
  if (it != pipelines_.end()) {
    return &it->second;
  }
  return nullptr;
}

// Variable interpolation: replace {{var}} with
// values from vars map
std::string PipelineExecutor::Interpolate(
    const std::string& tmpl,
    const std::map<std::string, nlohmann::json>& vars) const {
  std::string result = tmpl;
  // Match {{variable_name}} patterns
  std::regex var_regex("\\{\\{([^}]+)\\}\\}");
  std::smatch match;
  std::string working = result;

  while (std::regex_search(working, match, var_regex)) {
    std::string var_name = match[1].str();
    std::string replacement;

    // Support dotted access: step_id.field
    size_t dot = var_name.find('.');
    if (dot != std::string::npos) {
      std::string base = var_name.substr(0, dot);
      std::string field = var_name.substr(dot + 1);
      auto it = vars.find(base);
      if (it != vars.end() && it->second.is_object() &&
          it->second.contains(field)) {
        auto& val = it->second[field];
        if (val.is_string()) {
          replacement = val.get<std::string>();
        } else {
          replacement = val.dump();
        }
      }
    } else {
      auto it = vars.find(var_name);
      if (it != vars.end()) {
        if (it->second.is_string()) {
          replacement = it->second.get<std::string>();
        } else {
          replacement = it->second.dump();
        }
      }
    }

    // Replace first occurrence
    std::string target = "{{" + var_name + "}}";
    size_t pos = result.find(target);
    if (pos != std::string::npos) {
      result.replace(pos, target.size(), replacement);
    }

    // Move past this match
    working = match.suffix().str();
  }

  return result;
}

nlohmann::json PipelineExecutor::InterpolateJson(
    const nlohmann::json& j,
    const std::map<std::string, nlohmann::json>& vars) const {
  if (j.is_string()) {
    return Interpolate(j.get<std::string>(), vars);
  }
  if (j.is_object()) {
    nlohmann::json result = nlohmann::json::object();
    for (auto& [key, val] : j.items()) {
      result[key] = InterpolateJson(val, vars);
    }
    return result;
  }
  if (j.is_array()) {
    nlohmann::json result = nlohmann::json::array();
    for (auto& val : j) {
      result.push_back(InterpolateJson(val, vars));
    }
    return result;
  }
  return j;  // numbers, bools, null
}

bool PipelineExecutor::EvalCondition(
    const std::string& expr,
    const std::map<std::string, nlohmann::json>& vars) const {
  // Interpolate variables in expression
  std::string resolved = Interpolate(expr, vars);

  // Support operators: ==, !=, contains
  // Format: "left == right" or
  //         "left != right" or
  //         "left contains right"

  // Try == operator
  size_t pos = resolved.find(" == ");
  if (pos != std::string::npos) {
    std::string left = resolved.substr(0, pos);
    std::string right = resolved.substr(pos + 4);
    return left == right;
  }

  // Try != operator
  pos = resolved.find(" != ");
  if (pos != std::string::npos) {
    std::string left = resolved.substr(0, pos);
    std::string right = resolved.substr(pos + 4);
    return left != right;
  }

  // Try contains operator
  pos = resolved.find(" contains ");
  if (pos != std::string::npos) {
    std::string left = resolved.substr(0, pos);
    std::string right = resolved.substr(pos + 10);
    return left.find(right) != std::string::npos;
  }

  // Default: treat non-empty as true
  return !resolved.empty() && resolved != "false" && resolved != "0" &&
         resolved != "null";
}

std::string PipelineExecutor::ExecuteStep(
    const PipelineStep& step, std::map<std::string, nlohmann::json>& vars,
    const std::string& session_id) {
  std::string result;

  if (step.type == PipelineStepType::kTool) {
    // Interpolate args
    nlohmann::json resolved_args = InterpolateJson(step.args, vars);

    LOG(INFO) << "Pipeline step [" << step.id << "]: tool=" << step.tool_name;

    // Route through AgentCore's tool dispatch
    // (reuse existing ExecuteSkill, ExecuteCode,
    //  ExecuteFileOp, etc.)
    if (step.tool_name == "execute_code") {
      std::string code = resolved_args.value("code", "");
      nlohmann::json call_args = {{"code", code}};
      result = agent_->ExecuteSessionOp("create_session",  // dummy — we call
                                        call_args,         // directly below
                                        session_id);
      // Actually call via ProcessPrompt
      // with tool instruction
      std::string prompt =
          "Use the execute_code tool with "
          "this code: " +
          code;
      result = agent_->ProcessPrompt("pipeline_" + session_id, prompt);
    } else {
      // Generic: instruct LLM to use the tool
      std::string prompt =
          "Use the " + step.tool_name +
          " tool with these arguments: " + resolved_args.dump();
      result = agent_->ProcessPrompt("pipeline_" + session_id, prompt);
    }
  } else if (step.type == PipelineStepType::kPrompt) {
    // Interpolate prompt text
    std::string resolved_prompt = Interpolate(step.prompt, vars);

    LOG(INFO) << "Pipeline step [" << step.id << "]: prompt ("
              << resolved_prompt.size() << " chars)";

    result = agent_->ProcessPrompt("pipeline_" + session_id, resolved_prompt);
  }

  return result;
}

PipelineRunResult PipelineExecutor::RunPipeline(
    const std::string& pipeline_id, const nlohmann::json& input_vars) {
  PipelineRunResult run_result;
  run_result.pipeline_id = pipeline_id;

  auto start = std::chrono::steady_clock::now();

  Pipeline pipeline;
  {
    std::lock_guard<std::mutex> lock(pipelines_mutex_);
    auto it = pipelines_.find(pipeline_id);
    if (it == pipelines_.end()) {
      run_result.status = "failed";
      return run_result;
    }
    pipeline = it->second;  // Copy
  }

  LOG(INFO) << "Running pipeline: " << pipeline.name << " (" << pipeline.id
            << ")";

  AuditLogger::Instance().Log(AuditLogger::MakeEvent(
      AuditEventType::kToolExecution, "",
      {{"operation", "run_pipeline"}, {"pipeline", pipeline.name}}));

  // Initialize variables with input
  std::map<std::string, nlohmann::json> vars;
  if (input_vars.is_object()) {
    for (auto& [key, val] : input_vars.items()) {
      vars[key] = val;
    }
  }

  // Session for this pipeline run
  std::string session_id = "pipeline_" + pipeline_id;

  // Execute steps sequentially
  bool all_success = true;
  size_t step_idx = 0;

  while (step_idx < pipeline.steps.size()) {
    auto& step = pipeline.steps[step_idx];

    // Handle condition step
    if (step.type == PipelineStepType::kCondition) {
      bool cond_result = EvalCondition(step.condition, vars);

      LOG(INFO) << "Pipeline condition [" << step.id << "]: " << step.condition
                << " = " << (cond_result ? "true" : "false");

      // Store condition result
      std::string out_var = step.output_var.empty() ? step.id : step.output_var;
      vars[out_var] = cond_result;

      // Jump to target step
      std::string target = cond_result ? step.then_step : step.else_step;

      if (!target.empty()) {
        // Find step index by ID
        bool found = false;
        for (size_t i = 0; i < pipeline.steps.size(); i++) {
          if (pipeline.steps[i].id == target) {
            step_idx = i;
            found = true;
            break;
          }
        }
        if (found) {
          continue;  // Jump to target
        }
        LOG(WARNING) << "Pipeline: jump target not " << "found: " << target;
      }

      step_idx++;
      continue;
    }

    // Execute tool or prompt step
    int retries = 0;
    std::string result;
    bool step_success = false;

    do {
      try {
        result = ExecuteStep(step, vars, session_id);
        step_success = true;
      } catch (const std::exception& e) {
        LOG(WARNING) << "Pipeline step [" << step.id
                     << "] failed: " << e.what();
        result = std::string("Error: ") + e.what();
        retries++;
      }
    } while (!step_success && retries <= step.max_retries);

    // Store result as variable
    std::string out_var = step.output_var.empty() ? step.id : step.output_var;

    try {
      vars[out_var] = nlohmann::json::parse(result);
    } catch (...) {
      vars[out_var] = result;
    }

    run_result.step_results.emplace_back(step.id, result);

    if (!step_success) {
      all_success = false;
      if (!step.skip_on_failure) {
        LOG(WARNING) << "Pipeline aborted at step: " << step.id;
        break;
      }
      LOG(INFO) << "Pipeline: skipping failed step: " << step.id;
    }

    step_idx++;
  }

  // Cleanup pipeline session
  agent_->ClearSession(session_id);

  auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                     std::chrono::steady_clock::now() - start)
                     .count();

  run_result.duration_ms = static_cast<int>(elapsed);
  run_result.variables = vars;

  if (all_success && step_idx >= pipeline.steps.size()) {
    run_result.status = "success";
  } else if (step_idx >= pipeline.steps.size()) {
    run_result.status = "partial";
  } else {
    run_result.status = "failed";
  }

  LOG(INFO) << "Pipeline completed: " << pipeline.name
            << " status=" << run_result.status << " duration=" << elapsed
            << "ms";

  return run_result;
}

}  // namespace tizenclaw
