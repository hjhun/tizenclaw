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
#include "skill_repository.hh"

#include <filesystem>
#include <fstream>

#include "../../common/logging.hh"

namespace tizenclaw {

bool SkillRepository::Initialize(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "SkillRepository config not found, "
              << "disabled";
    return true;  // Non-fatal
  }

  try {
    nlohmann::json config;
    f >> config;
    enabled_ = config.value("enabled", false);
    repo_url_ =
        config.value("repository_url", "");
    verify_checksums_ =
        config.value("verify_checksums", true);

    if (enabled_ && !repo_url_.empty()) {
      LOG(INFO) << "SkillRepository enabled: "
                << repo_url_;
    } else {
      LOG(INFO) << "SkillRepository disabled";
    }
  } catch (const std::exception& e) {
    LOG(WARNING) << "SkillRepository config error: "
                 << e.what();
  }

  return true;
}

std::vector<SkillRepository::SkillInfo>
SkillRepository::SearchSkills(
    const std::string& query) const {
  if (!enabled_) return {};

  // Stub: In a real implementation, this would
  // make an HTTP GET to repo_url_/api/skills?q=query
  LOG(INFO) << "SkillRepository::SearchSkills: "
            << query << " (stub)";
  return {};
}

SkillRepository::InstallResult
SkillRepository::InstallSkill(
    const std::string& name,
    const std::string& version) {
  if (!enabled_) {
    return {false,
            "Skill repository is disabled", ""};
  }

  // Stub: In a real implementation, this would:
  // 1. GET /api/skills/{name}/{version}/download
  // 2. Verify checksum
  // 3. Check compatibility
  // 4. Extract to skills directory
  LOG(INFO) << "SkillRepository::InstallSkill: "
            << name << "@" << version << " (stub)";
  return {false,
          "Remote installation not yet implemented",
          ""};
}

std::vector<SkillRepository::SkillUpdate>
SkillRepository::CheckUpdates() const {
  if (!enabled_) return {};

  // Stub
  LOG(INFO)
      << "SkillRepository::CheckUpdates (stub)";
  return {};
}

bool SkillRepository::UninstallSkill(
    const std::string& name) {
  const std::string skills_dir =
      "/opt/usr/share/tizenclaw/tools/skills";
  std::string skill_path = skills_dir + "/" + name;

  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::exists(skill_path, ec)) {
    LOG(WARNING) << "Skill not found: " << name;
    return false;
  }

  fs::remove_all(skill_path, ec);
  if (ec) {
    LOG(ERROR) << "Failed to remove skill: "
               << ec.message();
    return false;
  }

  LOG(INFO) << "Skill uninstalled: " << name;
  return true;
}

nlohmann::json
SkillRepository::ListInstalledSkills() const {
  nlohmann::json result = nlohmann::json::array();
  const std::string skills_dir =
      "/opt/usr/share/tizenclaw/tools/skills";

  namespace fs = std::filesystem;
  std::error_code ec;
  if (!fs::is_directory(skills_dir, ec)) return result;

  for (const auto& entry :
       fs::directory_iterator(skills_dir, ec)) {
    if (!entry.is_directory()) continue;
    std::string manifest_path =
        entry.path() / "manifest.json";
    std::ifstream mf(manifest_path);
    if (!mf.is_open()) continue;

    try {
      nlohmann::json j;
      mf >> j;
      nlohmann::json info = {
          {"name", j.value("name",
              entry.path().filename().string())},
          {"version", j.value("version", "1.0.0")},
          {"description",
           j.value("description", "")},
          {"manifest_version",
           GetManifestVersion(j)},
      };
      // Include v2 fields if present
      if (j.contains("author"))
        info["author"] = j["author"];
      if (j.contains("compatibility"))
        info["compatibility"] = j["compatibility"];
      result.push_back(info);
    } catch (...) {
      continue;
    }
  }

  return result;
}

bool SkillRepository::VerifyChecksum(
    const std::string& file_path,
    const std::string& expected_sha256) const {
  // Stub: would compute SHA-256 of file_path
  // and compare to expected_sha256
  (void)file_path;
  (void)expected_sha256;
  return true;
}

bool SkillRepository::CheckCompatibility(
    const nlohmann::json& manifest) const {
  if (!manifest.contains("compatibility"))
    return true;  // v1 manifests always compatible

  auto compat = manifest["compatibility"];
  // Stub: would check min_daemon_version, platform
  if (compat.contains("min_daemon_version")) {
    LOG(INFO) << "Compatibility check: "
              << "min_daemon_version="
              << compat["min_daemon_version"];
  }
  return true;
}

int SkillRepository::GetManifestVersion(
    const nlohmann::json& manifest) {
  return manifest.value("manifest_version", 1);
}

}  // namespace tizenclaw
