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

#ifndef SKILL_PLUGIN_MANAGER_HH
#define SKILL_PLUGIN_MANAGER_HH

#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "../infra/pkgmgr_client.hh"

namespace tizenclaw {

// Manages skill injection from RPK packages.
// Listens for pkgmgr events and creates symlinks (or copies)
// from RPK lib/<skill_name>/ directories into
// /opt/usr/share/tizen-tools/skills/.
class SkillPluginManager : public PkgmgrClient::IListener {
 public:
  static SkillPluginManager& GetInstance();

  bool Initialize();
  void Shutdown();

  using ChangeCallback = std::function<void()>;
  void SetChangeCallback(ChangeCallback cb) { change_callback_ = cb; }

  // Get all skill names installed from RPK packages
  std::set<std::string> GetInstalledSkillDirs() const;

  // Parse skill names from metadata value (supports | delimiter)
  static std::vector<std::string> ParseSkillNames(const std::string& value);

  // Link or copy a skill directory from RPK to skills dir
  bool LinkSkillDir(const std::string& source, const std::string& target);

  // Remove a skill directory symlink or copy
  void RemoveSkillDir(const std::string& target);

 private:
  SkillPluginManager();
  ~SkillPluginManager();

  SkillPluginManager(const SkillPluginManager&) = delete;
  SkillPluginManager& operator=(const SkillPluginManager&) = delete;

  void OnPkgmgrEvent(std::shared_ptr<PkgmgrEventArgs> args) override;

  void HandleInstallEvent(const std::string& pkgid);
  void HandleUpdateEvent(const std::string& pkgid);
  void HandleUninstallEvent(const std::string& pkgid);

  bool LoadSkillsFromPkg(const std::string& pkgid);
  void UnloadSkillsFromPkg(const std::string& pkgid);

  // Collect all metadata values for the skill key from a package
  static std::vector<std::string> CollectSkillMetadata(
      const std::string& pkgid);

  std::mutex map_mutex_;
  std::map<std::string, std::shared_ptr<PkgmgrEventArgs>> package_events_;

  // pkgid -> list of installed skill directory names
  mutable std::mutex skills_mutex_;
  std::map<std::string, std::vector<std::string>> pkg_skills_;

  ChangeCallback change_callback_;

  static constexpr const char* kSkillsDir =
      "/opt/usr/share/tizen-tools/skills";
  static constexpr const char* kMetadataKey =
      "http://tizen.org/metadata/tizenclaw/skill";
};

}  // namespace tizenclaw

#endif  // SKILL_PLUGIN_MANAGER_HH
