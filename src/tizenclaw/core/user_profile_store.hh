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
#ifndef USER_PROFILE_STORE_HH_
#define USER_PROFILE_STORE_HH_

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

// User role definition for permission scoping
enum class UserRole {
  kGuest = 0,    // Limited access (e.g., read-only, media control)
  kChild = 1,    // Restricted access (safety bounds enforced strictly)
  kMember = 2,   // Standard access (home member)
  kAdmin = 3     // Full access (owner)
};

// Represents an individual user's profile and preferences
struct UserProfile {
  std::string user_id;
  std::string name;
  UserRole role = UserRole::kGuest;
  
  // Custom preferences (e.g., temperature scale, language, favorite settings)
  nlohmann::json preferences;

  // Optional: Voice embedding ID or identifying traits for recognition
  std::string voice_id;
};

// Manages user profiles, role mapping, and persistent storage.
// Used by AgentCore to determine context and by SafetyGuard
// to enforce role-based access control (RBAC).
class UserProfileStore {
 public:
  UserProfileStore() = default;
  ~UserProfileStore() = default;

  // Initialize and load profiles from a JSON file
  [[nodiscard]] bool Initialize(const std::string& db_path);

  // Get a user profile by ID. Returns a guest profile if not found.
  [[nodiscard]] UserProfile GetProfile(
      const std::string& user_id) const;

  // Add or update a user profile.
  // Returns true if successfully saved.
  bool UpsertProfile(const UserProfile& profile);

  // Delete a user profile
  bool DeleteProfile(const std::string& user_id);

  // Resolve a session_id to a user_id based on recent mapping
  std::string GetUserIdForSession(const std::string& session_id) const;

  // Bind a session_id to a specific user_id
  void BindSession(const std::string& session_id, const std::string& user_id);

  // Helper to convert Role to string
  static std::string RoleToString(UserRole role);
  static UserRole StringToRole(const std::string& role_str);

 private:
  // Save current profiles to disk
  bool SaveToDisk() const;

  std::map<std::string, UserProfile> profiles_;
  std::map<std::string, std::string> session_user_map_;
  
  std::string db_path_;
  mutable std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // USER_PROFILE_STORE_HH_
