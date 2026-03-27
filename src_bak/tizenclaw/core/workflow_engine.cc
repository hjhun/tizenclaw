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
#include "workflow_engine.hh"

#include <sys/stat.h>

#include <atomic>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <regex>
#include <sstream>

#include "../../common/logging.hh"
#include "../storage/audit_logger.hh"
#include "agent_core.hh"

namespace tizenclaw {

namespace {

constexpr const char* kFrontmatterDelim = "---";

// Trim leading/trailing whitespace
std::string Trim(const std::string& s) {
  auto start = s.find_first_not_of(" \t\r\n");
  if (start == std::string::npos) return "";
  auto end = s.find_last_not_of(" \t\r\n");
  return s.substr(start, end - start + 1);
}

// Extract value from "- key: value" line
std::string ExtractMetaValue(const std::string& line,
                             const std::string& key) {
  std::string prefix = "- " + key + ":";
  auto pos = line.find(prefix);
  if (pos == std::string::npos) return "";
  return Trim(line.substr(pos + prefix.size()));
}

}  // namespace

WorkflowEngine::WorkflowEngine(AgentCore* agent)
    : agent_(agent) {}

std::string WorkflowEngine::GetWorkflowsDir() const {
  return std::string(kDataDir) + "/workflows";
}

std::string WorkflowEngine::GenerateWorkflowId() {
  static std::atomic<int> counter{0};
  auto now = std::chrono::system_clock::now();
  auto ts =
      std::chrono::duration_cast<std::chrono::milliseconds>(
          now.time_since_epoch())
          .count();
  int seq = counter.fetch_add(1);
  std::ostringstream oss;
  oss << "wf-" << std::hex << ts << "-" << seq;
  return oss.str();
}

std::string WorkflowEngine::StepTypeToString(
    WorkflowStepType type) {
  switch (type) {
    case WorkflowStepType::kPrompt:
      return "prompt";
    case WorkflowStepType::kTool:
      return "tool";
    default:
      return "prompt";
  }
}

WorkflowStepType WorkflowEngine::StringToStepType(
    const std::string& s) {
  if (s == "tool") return WorkflowStepType::kTool;
  return WorkflowStepType::kPrompt;
}

nlohmann::json WorkflowEngine::ParseFrontmatter(
    const std::string& yaml_block) const {
  nlohmann::json result;
  std::istringstream iss(yaml_block);
  std::string line;

  while (std::getline(iss, line)) {
    line = Trim(line);
    if (line.empty()) continue;

    auto colon_pos = line.find(':');
    if (colon_pos == std::string::npos) continue;

    std::string key = Trim(line.substr(0, colon_pos));
    std::string value =
        Trim(line.substr(colon_pos + 1));
    if (!key.empty() && !value.empty())
      result[key] = value;
  }

  return result;
}

WorkflowStep WorkflowEngine::ParseStepBlock(
    const std::string& id, const std::string& title,
    const std::string& body) const {
  WorkflowStep step;
  step.id = id;
  step.description = title;
  step.type = WorkflowStepType::kPrompt;  // default

  std::istringstream iss(body);
  std::string line;
  bool in_multiline = false;
  std::string multiline_key;
  std::ostringstream multiline_value;

  while (std::getline(iss, line)) {
    std::string trimmed = Trim(line);

    // Handle multiline values (lines starting with
    // spaces after a "- key: |" declaration)
    if (in_multiline) {
      if (trimmed.starts_with("- ") &&
          trimmed.find(':') != std::string::npos) {
        // New meta key found, end multiline
        in_multiline = false;
        std::string val = Trim(multiline_value.str());
        if (multiline_key == "instruction")
          step.instruction = val;
        multiline_value.str("");
        multiline_value.clear();
      } else {
        multiline_value << line << "\n";
        continue;
      }
    }

    if (trimmed.empty()) continue;

    // Parse "- type: value"
    std::string val;
    val = ExtractMetaValue(trimmed, "type");
    if (!val.empty()) {
      step.type = StringToStepType(val);
      continue;
    }

    val = ExtractMetaValue(trimmed, "instruction");
    if (!val.empty()) {
      if (val == "|") {
        in_multiline = true;
        multiline_key = "instruction";
        continue;
      }
      step.instruction = val;
      continue;
    }

    val = ExtractMetaValue(trimmed, "tool_name");
    if (!val.empty()) {
      step.tool_name = val;
      continue;
    }

    val = ExtractMetaValue(trimmed, "output_var");
    if (!val.empty()) {
      step.output_var = val;
      continue;
    }

    val = ExtractMetaValue(trimmed, "skip_on_failure");
    if (!val.empty()) {
      step.skip_on_failure = (val == "true");
      continue;
    }

    val = ExtractMetaValue(trimmed, "max_retries");
    if (!val.empty()) {
      try {
        step.max_retries = std::stoi(val);
      } catch (...) {
        step.max_retries = 0;
      }
      continue;
    }

    val = ExtractMetaValue(trimmed, "args");
    if (!val.empty()) {
      try {
        step.args = nlohmann::json::parse(val);
      } catch (...) {
        step.args = nlohmann::json::object();
      }
      continue;
    }
  }

  // Flush remaining multiline content
  if (in_multiline) {
    std::string val = Trim(multiline_value.str());
    if (multiline_key == "instruction")
      step.instruction = val;
  }

  // Auto-assign output_var if not specified
  if (step.output_var.empty())
    step.output_var = step.id;

  return step;
}

Workflow WorkflowEngine::ParseMarkdown(
    const std::string& markdown) const {
  Workflow wf;
  wf.raw_markdown = markdown;

  // 1. Extract YAML frontmatter (between --- lines)
  std::string body = markdown;
  std::string trimmed_md = Trim(markdown);

  if (trimmed_md.starts_with(kFrontmatterDelim)) {
    auto first_delim = trimmed_md.find(kFrontmatterDelim);
    auto second_delim =
        trimmed_md.find(kFrontmatterDelim,
                        first_delim + 3);

    if (second_delim != std::string::npos) {
      std::string yaml_content =
          trimmed_md.substr(first_delim + 3,
                            second_delim - first_delim - 3);
      auto fm = ParseFrontmatter(yaml_content);

      wf.name = fm.value("name", "");
      wf.description = fm.value("description", "");
      wf.trigger = fm.value("trigger", "manual");

      body = trimmed_md.substr(second_delim + 3);
    }
  }

  // 2. Split body by "## Step N:" headers
  std::regex step_regex(
      R"(##\s+Step\s+(\d+)\s*:\s*(.*))",
      std::regex::icase);

  std::vector<std::tuple<std::string, std::string,
                         std::string>>
      raw_steps;  // (step_num, title, body)

  std::sregex_iterator it(body.begin(), body.end(),
                          step_regex);
  std::sregex_iterator end;
  std::vector<std::pair<size_t, std::smatch>> matches;

  for (; it != end; ++it) {
    matches.emplace_back(it->position(), *it);
  }

  for (size_t i = 0; i < matches.size(); i++) {
    auto& [pos, match] = matches[i];
    std::string step_num = match[1].str();
    std::string title = Trim(match[2].str());

    size_t content_start = pos + match[0].length();
    size_t content_end =
        (i + 1 < matches.size())
            ? matches[i + 1].first
            : body.size();

    std::string step_body =
        body.substr(content_start,
                    content_end - content_start);

    std::string step_id = "step_" + step_num;
    wf.steps.push_back(
        ParseStepBlock(step_id, title, step_body));
  }

  return wf;
}

void WorkflowEngine::LoadWorkflows() {
  std::string dir = GetWorkflowsDir();
  std::filesystem::path dir_path(dir);

  if (!std::filesystem::exists(dir_path)) {
    std::filesystem::create_directories(dir_path);
    return;
  }

  {
    std::lock_guard<std::mutex> lock(workflows_mutex_);
    workflows_.clear();

    for (const auto& entry :
         std::filesystem::directory_iterator(dir_path)) {
      if (!entry.is_regular_file()) continue;
      if (entry.path().extension() != ".md") continue;
      if (entry.path().filename() == "index.md") continue;

      try {
        Workflow wf = LoadWorkflowFile(
            entry.path().string());
        if (!wf.id.empty())
          workflows_[wf.id] = std::move(wf);
      } catch (const std::exception& e) {
        LOG(WARNING) << "Failed to load workflow: "
                     << entry.path().string() << ": "
                     << e.what();
      }
    }

    LOG(INFO) << "Loaded " << workflows_.size()
              << " workflows";
  }

  RegenerateIndex();
}

Workflow WorkflowEngine::LoadWorkflowFile(
    const std::string& path) const {
  std::ifstream f(path);
  if (!f.is_open())
    throw std::runtime_error("Cannot open: " + path);

  std::ostringstream oss;
  oss << f.rdbuf();
  f.close();

  std::string content = oss.str();

  // The first line should be a comment with the ID
  // Format: <!-- id: wf-xxx -->
  Workflow wf;
  std::regex id_regex(
      R"(<!--\s*id:\s*(\S+)\s*-->)");
  std::smatch id_match;
  if (std::regex_search(content, id_match, id_regex))
    wf.id = id_match[1].str();

  // Remove the ID comment for parsing
  std::string md_body =
      std::regex_replace(content, id_regex, "");

  Workflow parsed = ParseMarkdown(md_body);
  parsed.id = wf.id;
  parsed.raw_markdown = content;

  return parsed;
}

void WorkflowEngine::SaveWorkflow(
    const Workflow& w) const {
  std::string dir = GetWorkflowsDir();
  std::filesystem::create_directories(dir);

  std::string path = dir + "/" + w.id + ".md";

  // Build markdown content with embedded ID
  std::ostringstream oss;
  oss << "<!-- id: " << w.id << " -->\n";
  oss << w.raw_markdown;

  // Atomic write via temp file
  std::string tmp = path + ".tmp";
  std::ofstream f(tmp);
  if (f.is_open()) {
    f << oss.str();
    f.close();
    rename(tmp.c_str(), path.c_str());
  }
}

void WorkflowEngine::RegenerateIndex() const {
  std::string dir = GetWorkflowsDir();
  std::string path = dir + "/index.md";

  std::ostringstream oss;
  oss << "# Registered Workflows\n\n";

  {
    std::lock_guard<std::mutex> lock(workflows_mutex_);
    if (workflows_.empty()) {
      oss << "No workflows registered.\n";
    } else {
      oss << "| ID | Name | Description"
          << " | Steps | Trigger |\n";
      oss << "|----|------|------------"
          << "-|-------|---------|\n";
      for (const auto& [id, w] : workflows_) {
        oss << "| " << id
            << " | " << w.name
            << " | " << w.description
            << " | " << w.steps.size()
            << " | " << w.trigger
            << " |\n";
      }
    }
  }

  oss << "\n> Auto-generated by WorkflowEngine."
      << " Do not edit manually.\n";

  std::string tmp = path + ".tmp";
  std::ofstream f(tmp);
  if (f.is_open()) {
    f << oss.str();
    f.close();
    rename(tmp.c_str(), path.c_str());
  }
}

std::string WorkflowEngine::CreateWorkflow(
    const std::string& markdown) {
  Workflow wf = ParseMarkdown(markdown);

  if (wf.name.empty()) {
    LOG(WARNING) << "Workflow creation failed: "
                 << "name is required in frontmatter";
    return "";
  }

  if (wf.steps.empty()) {
    LOG(WARNING) << "Workflow creation failed: "
                 << "at least one step is required";
    return "";
  }

  wf.id = GenerateWorkflowId();

  {
    std::lock_guard<std::mutex> lock(workflows_mutex_);
    workflows_[wf.id] = wf;
  }

  SaveWorkflow(wf);
  RegenerateIndex();
  LOG(INFO) << "Created workflow: " << wf.id << " ("
            << wf.name << ") with " << wf.steps.size()
            << " steps";

  return wf.id;
}

nlohmann::json WorkflowEngine::ListWorkflows() const {
  std::lock_guard<std::mutex> lock(workflows_mutex_);
  nlohmann::json result = nlohmann::json::array();

  for (const auto& [id, w] : workflows_) {
    result.push_back(
        {{"id", id},
         {"name", w.name},
         {"description", w.description},
         {"trigger", w.trigger},
         {"steps_count",
          static_cast<int>(w.steps.size())}});
  }

  return result;
}

bool WorkflowEngine::DeleteWorkflow(
    const std::string& id) {
  {
    std::lock_guard<std::mutex> lock(workflows_mutex_);
    auto it = workflows_.find(id);
    if (it == workflows_.end()) return false;

    std::string path =
        GetWorkflowsDir() + "/" + id + ".md";
    std::filesystem::remove(path);

    workflows_.erase(it);
    LOG(INFO) << "Deleted workflow: " << id;
  }

  RegenerateIndex();
  return true;
}

const Workflow* WorkflowEngine::GetWorkflow(
    const std::string& id) const {
  std::lock_guard<std::mutex> lock(workflows_mutex_);
  auto it = workflows_.find(id);
  if (it != workflows_.end()) return &it->second;
  return nullptr;
}

std::string WorkflowEngine::Interpolate(
    const std::string& tmpl,
    const std::map<std::string, nlohmann::json>& vars)
    const {
  std::string result = tmpl;
  std::regex var_regex(R"(\{\{([^}]+)\}\})");
  std::smatch match;
  std::string working = result;

  while (std::regex_search(working, match, var_regex)) {
    std::string var_name = match[1].str();
    std::string replacement;

    auto it = vars.find(var_name);
    if (it != vars.end()) {
      if (it->second.is_string())
        replacement = it->second.get<std::string>();
      else
        replacement = it->second.dump();
    }

    std::string target = "{{" + var_name + "}}";
    auto pos = result.find(target);
    if (pos != std::string::npos)
      result.replace(pos, target.size(), replacement);

    working = match.suffix().str();
  }

  return result;
}

nlohmann::json WorkflowEngine::InterpolateJson(
    const nlohmann::json& j,
    const std::map<std::string, nlohmann::json>& vars)
    const {
  if (j.is_string())
    return Interpolate(j.get<std::string>(), vars);

  if (j.is_object()) {
    nlohmann::json result = nlohmann::json::object();
    for (const auto& [key, val] : j.items())
      result[key] = InterpolateJson(val, vars);
    return result;
  }

  if (j.is_array()) {
    nlohmann::json result = nlohmann::json::array();
    for (const auto& val : j)
      result.push_back(InterpolateJson(val, vars));
    return result;
  }

  return j;
}

std::string WorkflowEngine::ExecuteStep(
    const WorkflowStep& step,
    std::map<std::string, nlohmann::json>& vars,
    const std::string& session_id) {
  std::string result;

  if (step.type == WorkflowStepType::kTool) {
    nlohmann::json resolved_args =
        InterpolateJson(step.args, vars);

    LOG(INFO) << "Workflow step [" << step.id
              << "]: tool=" << step.tool_name;

    std::string prompt =
        "Use the " + step.tool_name +
        " tool with these arguments: " +
        resolved_args.dump();
    result = agent_->ProcessPrompt(
        "workflow_" + session_id, prompt);
  } else {
    // kPrompt: interpolate instruction and send to LLM
    std::string resolved_instruction =
        Interpolate(step.instruction, vars);

    LOG(INFO) << "Workflow step [" << step.id
              << "]: prompt ("
              << resolved_instruction.size()
              << " chars)";

    result = agent_->ProcessPrompt(
        "workflow_" + session_id,
        resolved_instruction);
  }

  return result;
}

WorkflowRunResult WorkflowEngine::RunWorkflow(
    const std::string& workflow_id,
    const nlohmann::json& input_vars) {
  WorkflowRunResult run_result;
  run_result.workflow_id = workflow_id;

  auto start = std::chrono::steady_clock::now();

  Workflow workflow;
  {
    std::lock_guard<std::mutex> lock(workflows_mutex_);
    auto it = workflows_.find(workflow_id);
    if (it == workflows_.end()) {
      run_result.status = "failed";
      return run_result;
    }
    workflow = it->second;  // Copy
  }

  LOG(INFO) << "Running workflow: " << workflow.name
            << " (" << workflow.id << ") with "
            << workflow.steps.size() << " steps";

  AuditLogger::Instance().Log(AuditLogger::MakeEvent(
      AuditEventType::kToolExecution, "",
      {{"operation", "run_workflow"},
       {"workflow", workflow.name}}));

  // Initialize variables with input
  std::map<std::string, nlohmann::json> vars;
  if (input_vars.is_object()) {
    for (const auto& [key, val] : input_vars.items())
      vars[key] = val;
  }

  std::string session_id =
      "workflow_" + workflow_id;

  // Execute steps sequentially
  bool all_success = true;
  size_t step_idx = 0;

  while (step_idx < workflow.steps.size()) {
    const auto& step = workflow.steps[step_idx];

    int retries = 0;
    std::string result;
    bool step_success = false;

    do {
      try {
        result = ExecuteStep(step, vars, session_id);
        step_success = true;
      } catch (const std::exception& e) {
        LOG(WARNING) << "Workflow step [" << step.id
                     << "] failed: " << e.what();
        result = std::string("Error: ") + e.what();
        retries++;
      }
    } while (!step_success &&
             retries <= step.max_retries);

    // Store result as variable
    std::string out_var =
        step.output_var.empty() ? step.id
                                : step.output_var;

    try {
      vars[out_var] = nlohmann::json::parse(result);
    } catch (...) {
      vars[out_var] = result;
    }

    run_result.step_results.emplace_back(
        step.id, result);

    if (!step_success) {
      all_success = false;
      if (!step.skip_on_failure) {
        LOG(WARNING) << "Workflow aborted at step: "
                     << step.id;
        break;
      }
      LOG(INFO) << "Workflow: skipping failed step: "
                << step.id;
    }

    step_idx++;
  }

  // Cleanup workflow session
  agent_->ClearSession(session_id);

  auto elapsed =
      std::chrono::duration_cast<
          std::chrono::milliseconds>(
          std::chrono::steady_clock::now() - start)
          .count();

  run_result.duration_ms = static_cast<int>(elapsed);
  run_result.variables = vars;

  if (all_success &&
      step_idx >= workflow.steps.size()) {
    run_result.status = "success";
  } else if (step_idx >= workflow.steps.size()) {
    run_result.status = "partial";
  } else {
    run_result.status = "failed";
  }

  LOG(INFO) << "Workflow completed: " << workflow.name
            << " status=" << run_result.status
            << " duration=" << elapsed << "ms";

  return run_result;
}

}  // namespace tizenclaw
