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
#ifndef SKILL_REPOSITORY_HH_
#define SKILL_REPOSITORY_HH_

#include <json.hpp>
#include <string>
#include <vector>

namespace tizenclaw {

// Remote skill repository client for searching,
// downloading, installing, and managing skills
// from a remote skill marketplace.
class SkillRepository {
 public:
  SkillRepository() = default;
  ~SkillRepository() = default;

  struct SkillInfo {
    std::string name;
    std::string version;
    std::string description;
    std::string author;
    std::string sha256;
    int manifest_version = 1;
  };

  struct InstallResult {
    bool success = false;
    std::string message;
    std::string installed_version;
  };

  struct SkillUpdate {
    std::string name;
    std::string current_version;
    std::string latest_version;
  };

  // Initialize with repository URL and config
  [[nodiscard]] bool Initialize(
      const std::string& config_path);

  // Search skills in remote repository
  [[nodiscard]] std::vector<SkillInfo> SearchSkills(
      const std::string& query) const;

  // Install a skill from repository
  [[nodiscard]] InstallResult InstallSkill(
      const std::string& name,
      const std::string& version = "latest");

  // Check for available updates
  [[nodiscard]] std::vector<SkillUpdate>
  CheckUpdates() const;

  // Uninstall a skill
  [[nodiscard]] bool UninstallSkill(
      const std::string& name);

  // List locally installed skills with versions
  [[nodiscard]] nlohmann::json
  ListInstalledSkills() const;

  // Check if enabled
  [[nodiscard]] bool IsEnabled() const {
    return enabled_;
  }

 private:
  // Verify SHA-256 checksum of downloaded file
  [[nodiscard]] bool VerifyChecksum(
      const std::string& file_path,
      const std::string& expected_sha256) const;

  // Check manifest compatibility
  [[nodiscard]] bool CheckCompatibility(
      const nlohmann::json& manifest) const;

  // Parse manifest v2 fields
  [[nodiscard]] static int GetManifestVersion(
      const nlohmann::json& manifest);

  bool enabled_ = false;
  std::string repo_url_;
  bool verify_checksums_ = true;
};

}  // namespace tizenclaw

#endif  // SKILL_REPOSITORY_HH_
