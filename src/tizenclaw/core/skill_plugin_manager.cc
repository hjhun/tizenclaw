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

#include "skill_plugin_manager.hh"

#include <pkgmgr-info.h>

#include <filesystem>
#include <fstream>
#include <sstream>

#include <json.hpp>
#include <sys/stat.h>

#include "../../common/logging.hh"
#include "skill_verifier.hh"

namespace tizenclaw {

SkillPluginManager& SkillPluginManager::GetInstance() {
  static SkillPluginManager instance;
  return instance;
}

SkillPluginManager::SkillPluginManager() {}

SkillPluginManager::~SkillPluginManager() { Shutdown(); }

bool SkillPluginManager::Initialize() {
  PkgmgrClient::GetInstance().AddListener(this);

  pkgmgrinfo_pkginfo_metadata_filter_h filter;
  int ret = pkgmgrinfo_pkginfo_metadata_filter_create(&filter);
  if (ret != PMINFO_R_OK) {
    LOG(ERROR) << "Failed to create metadata filter for skill: " << ret;
    return true;  // Graceful fallback for headless unit tests
  }

  pkgmgrinfo_pkginfo_metadata_filter_add(filter, kMetadataKey, nullptr);

  pkgmgrinfo_pkginfo_metadata_filter_foreach(
      filter,
      [](pkgmgrinfo_pkginfo_h handle, void* user_data) {
        auto* manager = static_cast<SkillPluginManager*>(user_data);
        char* pkgid = nullptr;
        if (pkgmgrinfo_pkginfo_get_pkgid(handle, &pkgid) == PMINFO_R_OK &&
            pkgid) {
          manager->LoadSkillsFromPkg(pkgid);
        }
        return 0;
      },
      this);

  pkgmgrinfo_pkginfo_metadata_filter_destroy(filter);

  LOG(INFO) << "SkillPluginManager initialized";
  return true;
}

void SkillPluginManager::Shutdown() {
  PkgmgrClient::GetInstance().RemoveListener(this);
  std::lock_guard<std::mutex> lock(skills_mutex_);
  pkg_skills_.clear();
}

std::set<std::string> SkillPluginManager::GetInstalledSkillDirs() const {
  std::lock_guard<std::mutex> lock(skills_mutex_);
  std::set<std::string> result;
  for (const auto& [pkgid, skills] : pkg_skills_) {
    for (const auto& skill : skills) {
      result.insert(skill);
    }
  }
  return result;
}

void SkillPluginManager::OnPkgmgrEvent(
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

void SkillPluginManager::HandleInstallEvent(const std::string& pkgid) {
  LOG(INFO) << "Skill install event for " << pkgid;
  LoadSkillsFromPkg(pkgid);
}

void SkillPluginManager::HandleUpdateEvent(const std::string& pkgid) {
  LOG(INFO) << "Skill update event for " << pkgid;
  UnloadSkillsFromPkg(pkgid);
  LoadSkillsFromPkg(pkgid);
}

void SkillPluginManager::HandleUninstallEvent(const std::string& pkgid) {
  LOG(INFO) << "Skill uninstall event for " << pkgid;
  UnloadSkillsFromPkg(pkgid);
}

std::vector<std::string> SkillPluginManager::ParseSkillNames(
    const std::string& value) {
  std::vector<std::string> names;
  std::istringstream ss(value);
  std::string token;
  while (std::getline(ss, token, '|')) {
    // Trim whitespace
    size_t start = token.find_first_not_of(" \t");
    size_t end = token.find_last_not_of(" \t");
    if (start != std::string::npos) {
      names.push_back(token.substr(start, end - start + 1));
    }
  }
  return names;
}

std::vector<std::string> SkillPluginManager::CollectSkillMetadata(
    const std::string& pkgid) {
  std::vector<std::string> all_skill_names;

  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret =
      pkgmgrinfo_pkginfo_get_usr_pkginfo(pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) {
    LOG(ERROR) << "Failed to get pkginfo for " << pkgid;
    return all_skill_names;
  }

  // Try single metadata value first (handles | delimiter)
  char* value = nullptr;
  ret = pkgmgrinfo_pkginfo_get_metadata_value(pkginfo, kMetadataKey, &value);
  if (ret == PMINFO_R_OK && value) {
    auto names = ParseSkillNames(value);
    all_skill_names.insert(all_skill_names.end(), names.begin(), names.end());
  }

  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
  return all_skill_names;
}

bool SkillPluginManager::LoadSkillsFromPkg(const std::string& pkgid) {
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

  // 2. Collect skill names from metadata
  auto skill_names = CollectSkillMetadata(pkgid);
  if (skill_names.empty()) {
    LOG(WARNING) << "No skill names found in metadata for " << pkgid;
    return false;
  }

  namespace fs = std::filesystem;
  std::vector<std::string> installed_skills;

  // 3. Create skills directory if needed
  std::error_code ec;
  fs::create_directories(kSkillsDir, ec);

  for (const auto& skill_name : skill_names) {
    // Source: {pkg_root}/lib/{skill_name}/
    std::string source = pkg_root + "/lib/" + skill_name;
    if (!fs::is_directory(source, ec)) {
      LOG(WARNING) << "Skill directory not found: " << source;
      continue;
    }

    // Check that manifest.json exists in source
    std::string manifest = source + "/manifest.json";
    if (!fs::exists(manifest, ec)) {
      LOG(WARNING) << "manifest.json not found in " << source;
      continue;
    }

    // Target: {kSkillsDir}/{pkgid}__{skill_name}
    std::string target_name = pkgid + "__" + skill_name;
    std::string target = std::string(kSkillsDir) + "/" + target_name;

    if (LinkSkillDir(source, target)) {
      // Set +x permission for native entry points
      std::string manifest = source + "/manifest.json";
      std::ifstream mfin(manifest);
      if (mfin.is_open()) {
        try {
          nlohmann::json mj;
          mfin >> mj;
          if (mj.value("runtime", "python") == "native") {
            std::string ep = mj.value(
                "entry_point", skill_name);
            std::string ep_path = target + "/" + ep;
            chmod(ep_path.c_str(), 0755);
          }
        } catch (...) {}
      }

      // Verify the skill
      auto result = SkillVerifier::Verify(target);
      if (!result.passed) {
        SkillVerifier::DisableSkill(target);
        LOG(WARNING) << "Skill " << skill_name
                     << " from " << pkgid
                     << " failed verification";
        for (const auto& e : result.errors) {
          LOG(WARNING) << "  error: " << e;
        }
      } else {
        SkillVerifier::EnableSkill(target);
      }

      installed_skills.push_back(target_name);
      LOG(INFO) << "Linked skill: " << skill_name << " from " << pkgid
                << " -> " << target;
    }
  }

  if (!installed_skills.empty()) {
    {
      std::lock_guard<std::mutex> lock(skills_mutex_);
      pkg_skills_[pkgid] = installed_skills;
    }

    if (change_callback_) {
      change_callback_();
    }
  }

  return !installed_skills.empty();
}

void SkillPluginManager::UnloadSkillsFromPkg(const std::string& pkgid) {
  std::vector<std::string> skill_dirs;
  {
    std::lock_guard<std::mutex> lock(skills_mutex_);
    auto it = pkg_skills_.find(pkgid);
    if (it == pkg_skills_.end()) return;
    skill_dirs = it->second;
    pkg_skills_.erase(it);
  }

  for (const auto& dir_name : skill_dirs) {
    std::string target = std::string(kSkillsDir) + "/" + dir_name;
    RemoveSkillDir(target);
    LOG(INFO) << "Removed skill dir: " << dir_name;
  }

  if (!skill_dirs.empty() && change_callback_) {
    change_callback_();
  }
}

bool SkillPluginManager::LinkSkillDir(const std::string& source,
                                      const std::string& target) {
  namespace fs = std::filesystem;
  std::error_code ec;

  // Remove existing target if any
  if (fs::exists(target, ec) || fs::is_symlink(target, ec)) {
    fs::remove_all(target, ec);
  }

  // Try symlink first
  fs::create_directory_symlink(source, target, ec);
  if (!ec) {
    return true;
  }

  LOG(WARNING) << "Symlink failed (" << ec.message()
               << "), falling back to copy: " << source;

  // Fallback: recursive copy
  ec.clear();
  fs::copy(source, target,
           fs::copy_options::recursive | fs::copy_options::overwrite_existing,
           ec);
  if (ec) {
    LOG(ERROR) << "Failed to copy skill dir: " << ec.message();
    return false;
  }

  return true;
}

void SkillPluginManager::RemoveSkillDir(const std::string& target) {
  namespace fs = std::filesystem;
  std::error_code ec;
  fs::remove_all(target, ec);
  if (ec) {
    LOG(WARNING) << "Failed to remove skill dir " << target << ": "
                 << ec.message();
  }
}

}  // namespace tizenclaw
