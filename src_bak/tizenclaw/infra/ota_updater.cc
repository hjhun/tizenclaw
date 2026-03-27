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
#include "ota_updater.hh"

#include <glib.h>

#include <algorithm>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <json.hpp>
#include <sstream>

#include "../common/logging.hh"
#include "http_client.hh"

namespace fs = std::filesystem;

namespace tizenclaw {

OtaUpdater::OtaUpdater(const std::string& skills_dir, ReloadCallback reload_cb)
    : skills_dir_(skills_dir), reload_cb_(std::move(reload_cb)) {
  backup_dir_ = skills_dir_ + "/.backup";
}

bool OtaUpdater::LoadConfig(const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "OTA config not found: " << config_path;
    return false;
  }

  try {
    nlohmann::json cfg;
    f >> cfg;

    if (cfg.contains("manifest_url"))
      manifest_url_ = cfg["manifest_url"].get<std::string>();

    if (cfg.contains("auto_check_interval_hours"))
      check_interval_hours_ = cfg["auto_check_interval_hours"].get<int>();

    if (cfg.contains("auto_update"))
      auto_update_ = cfg["auto_update"].get<bool>();

    LOG(INFO) << "OTA config loaded: url=" << manifest_url_;
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "OTA config parse error: " << e.what();
    return false;
  }
}

std::string OtaUpdater::ReadSkillVersion(const std::string& skill_dir) const {
  std::string manifest = skill_dir + "/manifest.json";
  std::ifstream f(manifest);
  if (!f.is_open()) return "0.0.0";

  try {
    nlohmann::json j;
    f >> j;
    if (j.contains("version")) return j["version"].get<std::string>();
  } catch (...) {
  }
  return "0.0.0";
}

bool OtaUpdater::IsNewerVersion(const std::string& local,
                                const std::string& remote) {
  auto parse = [](const std::string& v) -> std::vector<int> {
    std::vector<int> parts;
    std::istringstream iss(v);
    std::string token;
    while (std::getline(iss, token, '.')) {
      try {
        parts.push_back(std::stoi(token));
      } catch (...) {
        parts.push_back(0);
      }
    }
    while (parts.size() < 3) parts.push_back(0);
    return parts;
  };

  auto lv = parse(local);
  auto rv = parse(remote);

  for (size_t i = 0; i < 3; ++i) {
    if (rv[i] > lv[i]) return true;
    if (rv[i] < lv[i]) return false;
  }
  return false;  // Equal
}

std::vector<SkillUpdateInfo> OtaUpdater::ParseManifest(
    const std::string& manifest_json, const std::string& skills_dir) const {
  std::vector<SkillUpdateInfo> updates;

  try {
    auto manifest = nlohmann::json::parse(manifest_json);

    if (!manifest.contains("skills") || !manifest["skills"].is_array())
      return updates;

    for (const auto& skill : manifest["skills"]) {
      SkillUpdateInfo info;
      info.name = skill.value("name", "");
      info.remote_version = skill.value("version", "0.0.0");
      info.download_url = skill.value("url", "");
      info.sha256 = skill.value("sha256", "");

      if (info.name.empty()) continue;

      // Read local version
      std::string skill_dir = skills_dir + "/" + info.name;
      info.local_version = ReadSkillVersion(skill_dir);

      info.update_available =
          IsNewerVersion(info.local_version, info.remote_version);

      updates.push_back(std::move(info));
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "Manifest parse error: " << e.what();
  }

  return updates;
}

std::string OtaUpdater::CheckForUpdates() {
  if (manifest_url_.empty()) {
    return "{\"error\":"
           "\"No manifest URL configured\"}";
  }

  auto resp = HttpClient::Get(manifest_url_, {}, 2, 10, 30);
  if (!resp.success) {
    return "{\"error\":\"Failed to fetch "
           "manifest: " +
           resp.error + "\"}";
  }

  auto updates = ParseManifest(resp.body, skills_dir_);

  nlohmann::json result;
  result["manifest_url"] = manifest_url_;
  result["updates"] = nlohmann::json::array();

  for (const auto& u : updates) {
    nlohmann::json item = {{"name", u.name},
                           {"local_version", u.local_version},
                           {"remote_version", u.remote_version},
                           {"update_available", u.update_available}};
    result["updates"].push_back(item);
  }

  int available = std::count_if(
      updates.begin(), updates.end(),
      [](const SkillUpdateInfo& u) { return u.update_available; });
  result["available_count"] = available;

  return result.dump();
}

bool OtaUpdater::BackupSkill(const std::string& skill_name) {
  std::string src = skills_dir_ + "/" + skill_name;
  if (!fs::exists(src)) return true;

  // Create backup dir
  std::error_code ec;
  fs::create_directories(backup_dir_, ec);
  if (ec) {
    LOG(ERROR) << "Cannot create backup dir: " << ec.message();
    return false;
  }

  // Backup with timestamp
  auto now = std::time(nullptr);
  std::string backup_name = skill_name + "_" + std::to_string(now);
  std::string dest = backup_dir_ + "/" + backup_name;

  fs::copy(src, dest, fs::copy_options::recursive, ec);
  if (ec) {
    LOG(ERROR) << "Backup failed for " << skill_name << ": " << ec.message();
    return false;
  }

  LOG(INFO) << "Backed up " << skill_name << " to " << dest;
  return true;
}

bool OtaUpdater::DownloadFile(const std::string& url,
                              const std::string& dest_path) {
  auto resp = HttpClient::Get(url, {}, 3, 10, 120);
  if (!resp.success) {
    LOG(ERROR) << "Download failed: " << resp.error;
    return false;
  }

  std::ofstream f(dest_path, std::ios::binary);
  if (!f.is_open()) {
    LOG(ERROR) << "Cannot write to: " << dest_path;
    return false;
  }

  f.write(resp.body.c_str(), static_cast<std::streamsize>(resp.body.size()));
  return true;
}

bool OtaUpdater::VerifySha256(const std::string& file_path,
                              const std::string& expected) const {
  if (expected.empty()) return true;

  std::ifstream f(file_path, std::ios::binary);
  if (!f.is_open()) return false;

  GChecksum* cs = g_checksum_new(G_CHECKSUM_SHA256);
  if (!cs) return false;

  char buf[4096];
  while (f.read(buf, sizeof(buf)) || f.gcount() > 0) {
    g_checksum_update(cs, reinterpret_cast<const guchar*>(buf),
                      static_cast<gssize>(f.gcount()));
  }

  std::string actual = g_checksum_get_string(cs);
  g_checksum_free(cs);

  return actual == expected;
}

std::string OtaUpdater::UpdateSkill(const std::string& skill_name) {
  if (manifest_url_.empty()) {
    return "{\"error\":"
           "\"No manifest URL configured\"}";
  }

  // Fetch manifest
  auto resp = HttpClient::Get(manifest_url_, {}, 2, 10, 30);
  if (!resp.success) {
    return "{\"error\":\"Manifest fetch "
           "failed\"}";
  }

  auto updates = ParseManifest(resp.body, skills_dir_);

  // Find the skill
  const SkillUpdateInfo* target = nullptr;
  for (const auto& u : updates) {
    if (u.name == skill_name) {
      target = &u;
      break;
    }
  }

  if (!target) {
    return "{\"error\":\"Skill not found "
           "in manifest\"}";
  }

  if (!target->update_available) {
    return "{\"status\":\"up_to_date\","
           "\"version\":\"" +
           target->local_version + "\"}";
  }

  // Backup existing skill
  if (!BackupSkill(skill_name)) {
    return "{\"error\":\"Backup failed\"}";
  }

  // Download new version
  std::string tmp_file = "/tmp/tizenclaw_ota_" + skill_name;
  if (!DownloadFile(target->download_url, tmp_file)) {
    return "{\"error\":\"Download failed\"}";
  }

  // Verify checksum
  if (!VerifySha256(tmp_file, target->sha256)) {
    std::error_code ec;
    fs::remove(tmp_file, ec);
    return "{\"error\":\"SHA-256 mismatch\"}";
  }

  // Remove old skill dir & extract new
  std::string skill_dir = skills_dir_ + "/" + skill_name;
  std::error_code ec;
  fs::remove_all(skill_dir, ec);
  fs::create_directories(skill_dir, ec);

  // Move downloaded file as manifest
  // (In production this would be an archive
  //  extraction; for now we treat it as a
  //  direct manifest.json replacement)
  fs::rename(tmp_file, skill_dir + "/manifest.json", ec);
  if (ec) {
    LOG(ERROR) << "Install failed: " << ec.message();
    return "{\"error\":\"Install failed: " + ec.message() + "\"}";
  }

  // Trigger skill reload
  if (reload_cb_) reload_cb_();

  nlohmann::json result = {{"status", "updated"},
                           {"tool", skill_name},
                           {"old_version", target->local_version},
                           {"new_version", target->remote_version}};
  return result.dump();
}

std::string OtaUpdater::RollbackSkill(const std::string& skill_name) {
  // Find latest backup
  std::error_code ec;
  if (!fs::exists(backup_dir_, ec)) {
    return "{\"error\":"
           "\"No backups found\"}";
  }

  std::string latest_backup;
  std::filesystem::file_time_type latest_time{};

  for (const auto& entry : fs::directory_iterator(backup_dir_, ec)) {
    std::string name = entry.path().filename().string();
    if (name.find(skill_name + "_") == 0) {
      auto wt = entry.last_write_time(ec);
      if (!ec && (latest_backup.empty() || wt > latest_time)) {
        latest_backup = entry.path().string();
        latest_time = wt;
      }
    }
  }

  if (latest_backup.empty()) {
    return "{\"error\":\"No backup found "
           "for " +
           skill_name + "\"}";
  }

  // Remove current and restore backup
  std::string skill_dir = skills_dir_ + "/" + skill_name;
  fs::remove_all(skill_dir, ec);
  fs::copy(latest_backup, skill_dir, fs::copy_options::recursive, ec);

  if (ec) {
    return "{\"error\":\"Rollback failed: " + ec.message() + "\"}";
  }

  // Remove used backup
  fs::remove_all(latest_backup, ec);

  // Trigger skill reload
  if (reload_cb_) reload_cb_();

  std::string restored_ver = ReadSkillVersion(skill_dir);

  nlohmann::json result = {{"status", "rolled_back"},
                           {"tool", skill_name},
                           {"restored_version", restored_ver}};
  return result.dump();
}

}  // namespace tizenclaw
