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
#include "user_profile_store.hh"

#include <fstream>
#include <iostream>

#include "../../common/logging.hh"

namespace tizenclaw {

bool UserProfileStore::Initialize(const std::string& db_path) {
  std::lock_guard<std::mutex> lock(mutex_);
  db_path_ = db_path;

  std::ifstream f(db_path_);
  if (!f.is_open()) {
    LOG(INFO) << "UserProfileStore: no existing db, starting fresh at " << db_path_;
    return true; // OK to start empty
  }

  try {
    nlohmann::json j;
    f >> j;

    if (j.contains("profiles") && j["profiles"].is_array()) {
      for (const auto& item : j["profiles"]) {
        UserProfile p;
        p.user_id = item.value("user_id", "");
        p.name = item.value("name", "Unknown");
        p.role = StringToRole(item.value("role", "guest"));
        p.voice_id = item.value("voice_id", "");
        if (item.contains("preferences")) {
          p.preferences = item["preferences"];
        }

        if (!p.user_id.empty()) {
          profiles_[p.user_id] = std::move(p);
        }
      }
    }
    LOG(INFO) << "UserProfileStore: loaded " << profiles_.size() << " profiles.";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "UserProfileStore: failed to parse " << db_path_ << ": " << e.what();
    return false;
  }
}

UserProfile UserProfileStore::GetProfile(
    const std::string& user_id) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = profiles_.find(user_id);
  if (it != profiles_.end()) {
    return it->second;
  }
  // Return a generic guest profile
  UserProfile guest;
  guest.user_id = user_id.empty() ? "guest_default" : user_id;
  guest.name = "Guest";
  guest.role = UserRole::kGuest;
  return guest;
}

bool UserProfileStore::UpsertProfile(const UserProfile& profile) {
  if (profile.user_id.empty()) return false;

  {
    std::lock_guard<std::mutex> lock(mutex_);
    profiles_[profile.user_id] = profile;
  }
  return SaveToDisk();
}

bool UserProfileStore::DeleteProfile(const std::string& user_id) {
  {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = profiles_.find(user_id);
    if (it == profiles_.end()) return false;
    profiles_.erase(it);

    // Clean up session bindings
    for (auto sit = session_user_map_.begin(); sit != session_user_map_.end(); ) {
      if (sit->second == user_id) {
        sit = session_user_map_.erase(sit);
      } else {
        ++sit;
      }
    }
  }
  return SaveToDisk();
}

std::string UserProfileStore::GetUserIdForSession(
    const std::string& session_id) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = session_user_map_.find(session_id);
  if (it != session_user_map_.end()) {
    return it->second;
  }
  return ""; // Unbound session
}

void UserProfileStore::BindSession(
    const std::string& session_id, const std::string& user_id) {
  std::lock_guard<std::mutex> lock(mutex_);
  session_user_map_[session_id] = user_id;
  LOG(INFO) << "Bound session '" << session_id << "' to user '" << user_id << "'";
}

bool UserProfileStore::SaveToDisk() const {
  std::lock_guard<std::mutex> lock(mutex_);
  if (db_path_.empty()) return false;

  try {
    nlohmann::json j;
    nlohmann::json arr = nlohmann::json::array();
    for (const auto& [uid, p] : profiles_) {
      nlohmann::json obj;
      obj["user_id"] = p.user_id;
      obj["name"] = p.name;
      obj["role"] = RoleToString(p.role);
      obj["voice_id"] = p.voice_id;
      if (!p.preferences.empty()) {
        obj["preferences"] = p.preferences;
      }
      arr.push_back(std::move(obj));
    }
    j["profiles"] = arr;

    // Write to temporary file, then rename for atomicity
    std::string tmp_path = db_path_ + ".tmp";
    std::ofstream f(tmp_path);
    if (!f.is_open()) return false;
    f << j.dump(2) << std::endl;
    f.close();

    if (std::rename(tmp_path.c_str(), db_path_.c_str()) != 0) {
      return false;
    }
    return true;
  } catch (...) {
    return false;
  }
}

std::string UserProfileStore::RoleToString(UserRole role) {
  switch (role) {
    case UserRole::kGuest: return "guest";
    case UserRole::kChild: return "child";
    case UserRole::kMember: return "member";
    case UserRole::kAdmin: return "admin";
    default: return "guest";
  }
}

UserRole UserProfileStore::StringToRole(const std::string& role_str) {
  if (role_str == "admin") return UserRole::kAdmin;
  if (role_str == "member") return UserRole::kMember;
  if (role_str == "child") return UserRole::kChild;
  return UserRole::kGuest;
}

}  // namespace tizenclaw
