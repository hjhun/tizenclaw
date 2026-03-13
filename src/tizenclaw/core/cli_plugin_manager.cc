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

#include "cli_plugin_manager.hh"

#include <pkgmgr-info.h>

#include <filesystem>
#include <fstream>
#include <sstream>

#include <sys/stat.h>

#include "../../common/logging.hh"

namespace tizenclaw {

CliPluginManager& CliPluginManager::GetInstance() {
  static CliPluginManager instance;
  return instance;
}

CliPluginManager::CliPluginManager() {}

CliPluginManager::~CliPluginManager() { Shutdown(); }

bool CliPluginManager::Initialize() {
  PkgmgrClient::GetInstance().AddListener(this);

  pkgmgrinfo_pkginfo_metadata_filter_h filter;
  int ret = pkgmgrinfo_pkginfo_metadata_filter_create(&filter);
  if (ret != PMINFO_R_OK) {
    LOG(ERROR) << "Failed to create metadata filter for CLI: " << ret;
    return true;  // Graceful fallback for headless unit tests
  }

  pkgmgrinfo_pkginfo_metadata_filter_add(filter, kMetadataKey, nullptr);

  pkgmgrinfo_pkginfo_metadata_filter_foreach(
      filter,
      [](pkgmgrinfo_pkginfo_h handle, void* user_data) {
        auto* manager = static_cast<CliPluginManager*>(user_data);
        char* pkgid = nullptr;
        if (pkgmgrinfo_pkginfo_get_pkgid(handle, &pkgid) == PMINFO_R_OK &&
            pkgid) {
          manager->LoadCliFromPkg(pkgid);
        }
        return 0;
      },
      this);

  pkgmgrinfo_pkginfo_metadata_filter_destroy(filter);

  LOG(INFO) << "CliPluginManager initialized";
  return true;
}

void CliPluginManager::Shutdown() {
  PkgmgrClient::GetInstance().RemoveListener(this);
  std::lock_guard<std::mutex> lock(cli_mutex_);
  pkg_cli_tools_.clear();
}

std::set<std::string> CliPluginManager::GetInstalledCliDirs() const {
  std::lock_guard<std::mutex> lock(cli_mutex_);
  std::set<std::string> result;
  for (const auto& [pkgid, tools] : pkg_cli_tools_) {
    for (const auto& tool : tools) {
      result.insert(tool);
    }
  }
  return result;
}

void CliPluginManager::OnPkgmgrEvent(
    std::shared_ptr<PkgmgrEventArgs> args) {
  if (args->GetEventStatus() == "start") {
    std::lock_guard<std::mutex> lock(map_mutex_);
    package_events_[args->GetTag()] = args;
  } else if (args->GetEventStatus() == "end" &&
             args->GetEventName() == "ok") {
    std::shared_ptr<PkgmgrEventArgs> start_event;
    {
      std::lock_guard<std::mutex> lock(map_mutex_);
      auto it = package_events_.find(args->GetTag());
      if (it != package_events_.end()) {
        start_event = it->second;
        package_events_.erase(it);
      }
    }

    if (start_event) {
      const auto& evt = start_event->GetEventName();
      if (evt == "install" || evt == "recoverinstall") {
        HandleInstallEvent(args->GetPkgId());
      } else if (evt == "upgrade" || evt == "recoverupgrade") {
        HandleUpdateEvent(args->GetPkgId());
      } else if (evt == "uninstall" || evt == "recoveruninstall") {
        HandleUninstallEvent(args->GetPkgId());
      }
    }
  } else if (args->GetEventStatus() == "error") {
    std::lock_guard<std::mutex> lock(map_mutex_);
    package_events_.erase(args->GetTag());
  }
}

void CliPluginManager::HandleInstallEvent(const std::string& pkgid) {
  LOG(INFO) << "CLI install event for " << pkgid;
  LoadCliFromPkg(pkgid);
}

void CliPluginManager::HandleUpdateEvent(const std::string& pkgid) {
  LOG(INFO) << "CLI update event for " << pkgid;
  UnloadCliFromPkg(pkgid);
  LoadCliFromPkg(pkgid);
}

void CliPluginManager::HandleUninstallEvent(const std::string& pkgid) {
  LOG(INFO) << "CLI uninstall event for " << pkgid;
  UnloadCliFromPkg(pkgid);
}

std::vector<std::string> CliPluginManager::ParseCliNames(
    const std::string& value) {
  std::vector<std::string> names;
  std::istringstream ss(value);
  std::string token;
  while (std::getline(ss, token, '|')) {
    size_t start = token.find_first_not_of(" \t");
    size_t end = token.find_last_not_of(" \t");
    if (start != std::string::npos) {
      names.push_back(token.substr(start, end - start + 1));
    }
  }
  return names;
}

std::vector<std::string> CliPluginManager::CollectCliMetadata(
    const std::string& pkgid) {
  std::vector<std::string> all_cli_names;

  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret =
      pkgmgrinfo_pkginfo_get_usr_pkginfo(pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) {
    LOG(ERROR) << "Failed to get pkginfo for " << pkgid;
    return all_cli_names;
  }

  // Iterate over all apps in the package to collect CLI metadata
  pkgmgrinfo_appinfo_filter_h app_filter;
  ret = pkgmgrinfo_appinfo_filter_create(&app_filter);
  if (ret == PMINFO_R_OK) {
    pkgmgrinfo_appinfo_filter_add_string(
        app_filter, PMINFO_APPINFO_PROP_APP_PACKAGE, pkgid.c_str());

    struct AppIterCtx {
      std::vector<std::string>* names;
      const char* metadata_key;
    };
    AppIterCtx ctx{&all_cli_names, kMetadataKey};

    pkgmgrinfo_appinfo_usr_filter_foreach_appinfo(
        app_filter,
        [](pkgmgrinfo_appinfo_h handle, void* user_data) {
          auto* ctx = static_cast<AppIterCtx*>(user_data);
          char* value = nullptr;
          int ret = pkgmgrinfo_appinfo_get_metadata_value(
              handle, ctx->metadata_key, &value);
          if (ret == PMINFO_R_OK && value) {
            auto names = ParseCliNames(value);
            ctx->names->insert(ctx->names->end(),
                               names.begin(), names.end());
          }
          return 0;
        },
        &ctx, getuid());

    pkgmgrinfo_appinfo_filter_destroy(app_filter);
  }

  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
  return all_cli_names;
}

bool CliPluginManager::LoadCliFromPkg(const std::string& pkgid) {
  // 1. Get package root path
  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret =
      pkgmgrinfo_pkginfo_get_usr_pkginfo(pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) {
    LOG(ERROR) << "Failed to get pkginfo for " << pkgid;
    return false;
  }

  char* root_path = nullptr;
  ret = pkgmgrinfo_pkginfo_get_root_path(pkginfo, &root_path);
  if (ret != PMINFO_R_OK || !root_path) {
    LOG(ERROR) << "Failed to get root path for " << pkgid;
    pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
    return false;
  }
  std::string pkg_root(root_path);
  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);

  // 2. Collect CLI tool names from metadata
  auto cli_names = CollectCliMetadata(pkgid);
  if (cli_names.empty()) {
    LOG(WARNING) << "No CLI tool names found in metadata for " << pkgid;
    return false;
  }

  namespace fs = std::filesystem;
  std::vector<std::string> installed_tools;

  // 3. Create CLI tools directory if needed
  std::error_code ec;
  fs::create_directories(kCliDir, ec);

  for (const auto& cli_name : cli_names) {
    // Target: {kCliDir}/{pkgid}__{cli_name}
    std::string target_name = pkgid + "__" + cli_name;
    std::string target_dir = std::string(kCliDir) + "/" + target_name;

    if (LinkCliTool(pkg_root, cli_name, target_dir)) {
      installed_tools.push_back(target_name);
      LOG(INFO) << "Linked CLI tool: " << cli_name << " from " << pkgid
                << " -> " << target_dir;
    }
  }

  if (!installed_tools.empty()) {
    {
      std::lock_guard<std::mutex> lock(cli_mutex_);
      pkg_cli_tools_[pkgid] = installed_tools;
    }

    if (change_callback_) {
      change_callback_();
    }
  }

  return !installed_tools.empty();
}

void CliPluginManager::UnloadCliFromPkg(const std::string& pkgid) {
  std::vector<std::string> tool_dirs;
  {
    std::lock_guard<std::mutex> lock(cli_mutex_);
    auto it = pkg_cli_tools_.find(pkgid);
    if (it == pkg_cli_tools_.end()) return;
    tool_dirs = it->second;
    pkg_cli_tools_.erase(it);
  }

  for (const auto& dir_name : tool_dirs) {
    std::string target = std::string(kCliDir) + "/" + dir_name;
    RemoveCliDir(target);
    LOG(INFO) << "Removed CLI tool dir: " << dir_name;
  }

  if (!tool_dirs.empty() && change_callback_) {
    change_callback_();
  }
}

bool CliPluginManager::LinkCliTool(const std::string& pkg_root,
                                   const std::string& cli_name,
                                   const std::string& target_dir) {
  namespace fs = std::filesystem;
  std::error_code ec;

  // Remove existing target if any
  if (fs::exists(target_dir, ec) || fs::is_symlink(target_dir, ec)) {
    fs::remove_all(target_dir, ec);
  }

  // Create target directory
  fs::create_directories(target_dir, ec);
  if (ec) {
    LOG(ERROR) << "Failed to create CLI tool dir: " << ec.message();
    return false;
  }

  // Look for the executable binary.
  // TPK installs binaries to {pkg_root}/bin/{cli_name}
  std::string bin_path = pkg_root + "/bin/" + cli_name;
  if (!fs::exists(bin_path, ec)) {
    LOG(WARNING) << "CLI binary not found: " << bin_path;
    return false;
  }

  // Make sure binary is executable
  chmod(bin_path.c_str(), 0755);

  // Create symlink: target_dir/executable -> bin_path
  std::string exec_link = target_dir + "/executable";
  fs::create_symlink(bin_path, exec_link, ec);
  if (ec) {
    LOG(WARNING) << "Symlink failed for executable (" << ec.message()
                 << "), creating copy";
    fs::copy_file(bin_path, exec_link,
                  fs::copy_options::overwrite_existing, ec);
    if (ec) {
      LOG(ERROR) << "Failed to copy CLI binary: " << ec.message();
      return false;
    }
    chmod(exec_link.c_str(), 0755);
  }

  // Look for tool descriptor (.tool.md) in res/ directory
  // Convention: res/{cli_name}.tool.md
  std::string tool_md_path = pkg_root + "/res/" + cli_name + ".tool.md";
  if (fs::exists(tool_md_path, ec)) {
    std::string md_link = target_dir + "/tool.md";
    fs::create_symlink(tool_md_path, md_link, ec);
    if (ec) {
      fs::copy_file(tool_md_path, md_link,
                    fs::copy_options::overwrite_existing, ec);
    }
    LOG(INFO) << "Linked tool descriptor: " << cli_name << ".tool.md";
  } else {
    LOG(WARNING) << "No tool.md descriptor found for CLI: " << cli_name
                 << " (expected at " << tool_md_path << ")";
  }

  return true;
}

void CliPluginManager::RemoveCliDir(const std::string& target) {
  namespace fs = std::filesystem;
  std::error_code ec;
  fs::remove_all(target, ec);
  if (ec) {
    LOG(WARNING) << "Failed to remove CLI dir " << target << ": "
                 << ec.message();
  }
}

}  // namespace tizenclaw
