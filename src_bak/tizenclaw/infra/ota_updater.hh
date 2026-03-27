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
#ifndef OTA_UPDATER_HH
#define OTA_UPDATER_HH

#include <functional>
#include <string>
#include <vector>

namespace tizenclaw {

// Skill update info from remote manifest
struct SkillUpdateInfo {
  std::string name;
  std::string remote_version;
  std::string local_version;
  std::string download_url;
  std::string sha256;
  bool update_available = false;
};

// Over-the-air update mechanism for
// skills via remote manifest.
class OtaUpdater {
 public:
  using ReloadCallback = std::function<void()>;

  OtaUpdater(const std::string& skills_dir, ReloadCallback reload_cb);
  ~OtaUpdater() = default;

  // Load OTA config from JSON file
  [[nodiscard]] bool LoadConfig(const std::string& config_path);

  // Check remote manifest for updates
  // Returns list of available updates
  [[nodiscard]] std::string CheckForUpdates();

  // Update a specific skill by name
  [[nodiscard]] std::string UpdateSkill(const std::string& skill_name);

  // Rollback a skill to its backup
  [[nodiscard]] std::string RollbackSkill(const std::string& skill_name);

  // Get manifest URL
  [[nodiscard]] std::string GetManifestUrl() const { return manifest_url_; }

  // Parse a manifest JSON string
  // (public for testing)
  [[nodiscard]] std::vector<SkillUpdateInfo> ParseManifest(
      const std::string& manifest_json, const std::string& skills_dir) const;

  // Compare version strings (a < b)
  // Supports semver: "1.2.3" < "1.3.0"
  [[nodiscard]] static bool IsNewerVersion(const std::string& local,
                                           const std::string& remote);

 private:
  // Read version from skill manifest.json
  std::string ReadSkillVersion(const std::string& skill_dir) const;

  // Create backup of existing skill
  bool BackupSkill(const std::string& skill_name);

  // Download file to path
  bool DownloadFile(const std::string& url, const std::string& dest_path);

  // Verify SHA-256 of file
  bool VerifySha256(const std::string& file_path,
                    const std::string& expected) const;

  std::string skills_dir_;
  std::string backup_dir_;
  std::string manifest_url_;
  bool auto_update_ = false;
  int check_interval_hours_ = 24;
  ReloadCallback reload_cb_;
};

}  // namespace tizenclaw

#endif  // OTA_UPDATER_HH
