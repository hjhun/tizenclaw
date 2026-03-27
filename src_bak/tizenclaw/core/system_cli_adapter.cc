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

  // Auto-discover CLI tools from systemd service files
  if (auto_discover_) {
    ScanSystemdServices(systemd_dir_);
  }

  LoadToolDocs(tools_dir_);
  RegisterCapabilities();

  LOG(INFO) << "SystemCliAdapter initialized with "
            << tools_.size() << " tools"
            << (auto_discover_ ? " (auto-discover enabled)" : "");
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

    config_path_ = config_path;

    tools_dir_ = config.value("tools_dir",
        "/opt/usr/share/tizen-tools/system_cli");
    auto_discover_ = config.value("auto_discover", false);
    systemd_dir_ = config.value("systemd_dir",
        "/usr/lib/systemd/system");

    if (!config.contains("tools") || !config["tools"].is_object()) {
      LOG(INFO) << "SystemCliAdapter: no tools defined in config";
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

void SystemCliAdapter::ScanSystemdServices(
    const std::string& systemd_dir) {
  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::is_directory(systemd_dir, ec)) {
    LOG(WARNING) << "SystemCliAdapter: systemd dir not found: "
                 << systemd_dir;
    return;
  }

  int discovered = 0;
  // Collect discovered tools first, then merge under the lock.
  std::map<std::string, SystemCliToolConfig> discovered_tools;

  for (const auto& entry : fs::directory_iterator(systemd_dir, ec)) {
    if (!entry.is_regular_file()) continue;

    auto filename = entry.path().filename().string();
    if (filename.size() <= 8 ||
        filename.compare(filename.size() - 8, 8, ".service") != 0) {
      continue;
    }

    std::ifstream sf(entry.path());
    if (!sf.is_open()) continue;

    std::string line;
    bool in_service_section = false;
    while (std::getline(sf, line)) {
      // Trim leading/trailing whitespace
      size_t start = line.find_first_not_of(" \t");
      if (start == std::string::npos) continue;
      line = line.substr(start);

      if (line.front() == '[') {
        in_service_section = (line == "[Service]");
        continue;
      }

      if (!in_service_section) continue;

      static const std::string prefix = "ExecStart=";
      if (line.compare(0, prefix.size(), prefix) != 0) continue;

      std::string exec_value = line.substr(prefix.size());
      // Strip leading '-' (optional prefix in systemd)
      if (!exec_value.empty() && exec_value.front() == '-') {
        exec_value = exec_value.substr(1);
      }
      // Trim whitespace
      size_t pos = exec_value.find_first_not_of(" \t");
      if (pos == std::string::npos) continue;
      exec_value = exec_value.substr(pos);

      // Extract binary path (first space-delimited token)
      std::string bin_path = exec_value;
      size_t space_pos = exec_value.find(' ');
      if (space_pos != std::string::npos) {
        bin_path = exec_value.substr(0, space_pos);
      }

      if (bin_path.empty() || bin_path.front() != '/') continue;

      // If binary EXISTS on disk, it's a daemon — skip
      if (fs::exists(bin_path, ec)) continue;

      // Binary doesn't exist: likely a CLI tool candidate
      std::string tool_name = fs::path(bin_path).filename().string();
      if (discovered_tools.contains(tool_name)) continue;

      SystemCliToolConfig tool_cfg;
      tool_cfg.binary_path = bin_path;
      tool_cfg.timeout_seconds = 10;
      tool_cfg.side_effect = "unknown";
      tool_cfg.description =
          "Auto-discovered from " + filename;

      discovered_tools[tool_name] = std::move(tool_cfg);
      discovered++;
      LOG(INFO) << "SystemCliAdapter: auto-discovered '"
                << tool_name << "' from " << filename;
    }
  }

  // Merge discovered tools under the lock.
  // Config-defined tools take precedence.
  {
    std::lock_guard<std::mutex> lock(mutex_);
    for (auto& [name, cfg] : discovered_tools) {
      if (!tools_.contains(name)) {
        tools_[name] = std::move(cfg);
      }
    }
  }

  LOG(INFO) << "SystemCliAdapter: auto-discovered "
            << discovered << " tools from " << systemd_dir;
}

void SystemCliAdapter::RegisterToolCapability(
    const std::string& name, const SystemCliToolConfig& cfg) {
  auto& reg = CapabilityRegistry::GetInstance();

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

void SystemCliAdapter::RegisterCapabilities() {
  std::lock_guard<std::mutex> lock(mutex_);
  for (const auto& [name, cfg] : tools_) {
    RegisterToolCapability(name, cfg);
  }
}

std::string SystemCliAdapter::RegisterTool(
    const std::string& name,
    const SystemCliToolConfig& config,
    const std::string& tool_doc) {
  if (name.empty()) return "Tool name is required";
  if (config.binary_path.empty()) return "Binary path is required";

  {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!enabled_) return "SystemCliAdapter is not enabled";
    tools_[name] = config;
    if (!tool_doc.empty()) {
      tool_docs_[name] = tool_doc;
    }
  }

  // Write tool.md to tools_dir_
  if (!tool_doc.empty()) {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::create_directories(tools_dir_, ec);
    std::string md_path = tools_dir_ + "/" + name + ".tool.md";
    std::ofstream mf(md_path);
    if (mf.is_open()) {
      mf << tool_doc;
      mf.close();
      LOG(INFO) << "SystemCliAdapter: wrote " << md_path;
    }
  }

  // Register capability
  RegisterToolCapability(name, config);

  // Persist config
  SaveConfig();

  LOG(INFO) << "SystemCliAdapter: registered tool '" << name
            << "' -> " << config.binary_path;
  return "";
}

std::string SystemCliAdapter::UnregisterTool(
    const std::string& name) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    if (!enabled_) return "SystemCliAdapter is not enabled";
    if (!tools_.contains(name)) {
      return "Tool not found: " + name;
    }
    tools_.erase(name);
    tool_docs_.erase(name);
  }

  // Remove tool.md
  namespace fs = std::filesystem;
  std::error_code ec;
  std::string md_path = tools_dir_ + "/" + name + ".tool.md";
  fs::remove(md_path, ec);

  // Unregister capability
  CapabilityRegistry::GetInstance().Unregister("system_cli:" + name);

  // Persist config
  SaveConfig();

  LOG(INFO) << "SystemCliAdapter: unregistered tool '" << name << "'";
  return "";
}

nlohmann::json SystemCliAdapter::GetRegisteredToolsJson() const {
  std::lock_guard<std::mutex> lock(mutex_);
  nlohmann::json result = nlohmann::json::object();
  result["enabled"] = enabled_;
  result["tool_count"] = tools_.size();
  nlohmann::json tools_arr = nlohmann::json::array();
  for (const auto& [name, cfg] : tools_) {
    nlohmann::json t;
    t["name"] = name;
    t["path"] = cfg.binary_path;
    t["timeout_seconds"] = cfg.timeout_seconds;
    t["side_effect"] = cfg.side_effect;
    t["description"] = cfg.description;
    t["has_doc"] = tool_docs_.contains(name);
    if (!cfg.blocked_args.empty()) {
      t["blocked_args"] = cfg.blocked_args;
    }
    tools_arr.push_back(t);
  }
  result["tools"] = tools_arr;
  return result;
}

bool SystemCliAdapter::SaveConfig() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (config_path_.empty()) {
    LOG(WARNING) << "SystemCliAdapter: no config path set";
    return false;
  }

  nlohmann::json config;
  config["enabled"] = enabled_;
  config["auto_discover"] = auto_discover_;
  config["systemd_dir"] = systemd_dir_;
  config["tools_dir"] = tools_dir_;

  nlohmann::json tools_json = nlohmann::json::object();
  for (const auto& [name, cfg] : tools_) {
    nlohmann::json t;
    t["path"] = cfg.binary_path;
    t["timeout_seconds"] = cfg.timeout_seconds;
    t["side_effect"] = cfg.side_effect;
    t["description"] = cfg.description;
    if (!cfg.blocked_args.empty()) {
      t["blocked_args"] = cfg.blocked_args;
    }
    tools_json[name] = t;
  }
  config["tools"] = tools_json;

  std::ofstream f(config_path_);
  if (!f.is_open()) {
    LOG(ERROR) << "SystemCliAdapter: cannot write config: "
               << config_path_;
    return false;
  }
  f << config.dump(2) << "\n";
  f.close();

  LOG(INFO) << "SystemCliAdapter: config saved to " << config_path_;
  return true;
}

}  // namespace tizenclaw
