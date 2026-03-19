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

#include "system_cli_adapter.hh"

#include <json.hpp>

#include <filesystem>
#include <fstream>

#include "../../common/logging.hh"
#include "capability_registry.hh"

namespace tizenclaw {

SystemCliAdapter& SystemCliAdapter::GetInstance() {
  static SystemCliAdapter instance;
  return instance;
}

bool SystemCliAdapter::Initialize(const std::string& config_path) {
  if (!LoadConfig(config_path)) {
    LOG(WARNING) << "SystemCliAdapter: config not found or disabled ("
                 << config_path << ")";
    return true;  // Graceful: system CLI is optional
  }

  if (!enabled_) {
    LOG(INFO) << "SystemCliAdapter: disabled by config";
    return true;
  }

  LoadToolDocs(tools_dir_);
  RegisterCapabilities();

  LOG(INFO) << "SystemCliAdapter initialized with "
            << tools_.size() << " tools";
  return true;
}

void SystemCliAdapter::Shutdown() {
  std::lock_guard<std::mutex> lock(mutex_);
  tools_.clear();
  tool_docs_.clear();
  enabled_ = false;
}

bool SystemCliAdapter::HasTool(const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  return tools_.contains(name);
}

std::string SystemCliAdapter::Resolve(const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = tools_.find(name);
  if (it == tools_.end()) return "";
  return it->second.binary_path;
}

std::string SystemCliAdapter::ValidateArguments(
    const std::string& name, const std::string& arguments) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = tools_.find(name);
  if (it == tools_.end()) return "Tool not found: " + name;

  for (const auto& blocked : it->second.blocked_args) {
    // Check if any blocked pattern appears in the arguments
    if (arguments.find(blocked) != std::string::npos) {
      return "Blocked argument '" + blocked +
             "' is not allowed for tool '" + name + "'";
    }
  }
  return "";  // Valid
}

std::vector<std::string> SystemCliAdapter::GetToolNames() const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<std::string> names;
  names.reserve(tools_.size());
  for (const auto& [name, _] : tools_) {
    names.push_back(name);
  }
  return names;
}

std::map<std::string, std::string> SystemCliAdapter::GetToolDocs() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return tool_docs_;
}

int SystemCliAdapter::GetTimeout(const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = tools_.find(name);
  if (it == tools_.end()) return 10;
  return it->second.timeout_seconds;
}

bool SystemCliAdapter::IsEnabled() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return enabled_;
}

bool SystemCliAdapter::LoadConfig(const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) return false;

  try {
    auto config = nlohmann::json::parse(f);
    f.close();

    std::lock_guard<std::mutex> lock(mutex_);
    enabled_ = config.value("enabled", false);
    if (!enabled_) return true;

    tools_dir_ = config.value("tools_dir",
        "/opt/usr/share/tizenclaw/tools/system_cli");

    if (!config.contains("tools") || !config["tools"].is_object()) {
      LOG(WARNING) << "SystemCliAdapter: no tools defined in config";
      return true;
    }

    namespace fs = std::filesystem;
    for (auto& [name, tool_json] : config["tools"].items()) {
      SystemCliToolConfig tool_cfg;
      tool_cfg.binary_path = tool_json.value("path", "/usr/bin/" + name);
      tool_cfg.timeout_seconds = tool_json.value("timeout_seconds", 10);
      tool_cfg.side_effect = tool_json.value("side_effect", "none");
      tool_cfg.description = tool_json.value("description", name);

      if (tool_json.contains("blocked_args") &&
          tool_json["blocked_args"].is_array()) {
        for (const auto& arg : tool_json["blocked_args"]) {
          tool_cfg.blocked_args.push_back(arg.get<std::string>());
        }
      }

      // Validate binary exists
      std::error_code ec;
      if (!fs::exists(tool_cfg.binary_path, ec)) {
        LOG(WARNING) << "SystemCliAdapter: binary not found for '"
                     << name << "' at " << tool_cfg.binary_path
                     << ", skipping";
        continue;
      }

      tools_[name] = std::move(tool_cfg);
      LOG(INFO) << "SystemCliAdapter: registered tool '" << name
                << "' -> " << tools_[name].binary_path;
    }

    return true;
  } catch (const nlohmann::json::exception& e) {
    LOG(ERROR) << "SystemCliAdapter: config parse error: " << e.what();
    return false;
  }
}

void SystemCliAdapter::LoadToolDocs(const std::string& tools_dir) {
  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::is_directory(tools_dir, ec)) {
    LOG(WARNING) << "SystemCliAdapter: tools_dir not found: " << tools_dir;
    return;
  }

  std::lock_guard<std::mutex> lock(mutex_);

  for (const auto& entry : fs::directory_iterator(tools_dir, ec)) {
    if (!entry.is_regular_file()) continue;

    auto filename = entry.path().filename().string();
    // Match pattern: <tool_name>.tool.md
    static const std::string suffix = ".tool.md";
    if (filename.size() <= suffix.size() ||
        filename.compare(filename.size() - suffix.size(),
                         suffix.size(), suffix) != 0) {
      continue;
    }

    std::string tool_name =
        filename.substr(0, filename.size() - suffix.size());

    // Only load docs for tools that are in the whitelist
    if (!tools_.contains(tool_name)) continue;

    std::ifstream mf(entry.path());
    if (mf.is_open()) {
      std::string content(
          (std::istreambuf_iterator<char>(mf)),
          std::istreambuf_iterator<char>());
      if (!content.empty()) {
        tool_docs_[tool_name] = content;
        LOG(INFO) << "SystemCliAdapter: loaded docs for '" << tool_name << "'";
      }
    }
  }
}

void SystemCliAdapter::RegisterCapabilities() {
  auto& reg = CapabilityRegistry::GetInstance();

  // Lock is already held or we read from stable state
  for (const auto& [name, cfg] : tools_) {
    Capability cap;
    cap.name = "system_cli:" + name;
    cap.description = cfg.description;
    cap.category = "system_cli";
    cap.source = CapabilitySource::kSystemCli;
    cap.contract.execution_env = "host";
    cap.contract.estimated_duration_ms = cfg.timeout_seconds * 1000;

    if (cfg.side_effect == "reversible") {
      cap.contract.side_effect = SideEffect::kReversible;
    } else if (cfg.side_effect == "irreversible") {
      cap.contract.side_effect = SideEffect::kIrreversible;
    } else {
      cap.contract.side_effect = SideEffect::kNone;
    }

    reg.Register(cap.name, cap);
  }
}

}  // namespace tizenclaw
