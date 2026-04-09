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

#ifndef TIZENCLAW_CLI_SMARTTHINGS_CLIENT_HH_
#define TIZENCLAW_CLI_SMARTTHINGS_CLIENT_HH_

#include <string>

namespace tizenclaw {
namespace cli {

// OAuth2 credentials and device information loaded from config JSON.
struct VacuumConfig {
  std::string client_id;
  std::string client_secret;
  std::string access_token;
  std::string refresh_token;
  std::string device_id;
};

// HTTP client for the SmartThings REST API.
// Handles OAuth2 Bearer authentication with automatic token refresh on 401.
// All public methods return a valid JSON string suitable for stdout output.
class SmartThingsClient {
 public:
  // Loads credentials from the given JSON config file path.
  explicit SmartThingsClient(const std::string& config_path);

  // Sends one or more capability commands to the vacuum.
  // body must be a complete JSON string:
  //   {"commands":[{"component":"main","capability":"...","command":"...","arguments":["..."]}]}
  // Returns {"status":"ok",...} on HTTP 204, or {"status":"error","code":N} otherwise.
  std::string SendCommands(const std::string& body);

  // Fetches the device component/main/status and returns a parsed JSON subset:
  // {"status":"ok","battery_pct":N,"movement":"...","cleaning_mode":"...","turbo_mode":"..."}
  std::string GetStatus();

  const std::string& device_id() const { return config_.device_id; }

 private:
  // Attempts to refresh the access token using the stored refresh token.
  // Updates config_ and persists new tokens to disk on success.
  // Returns true on success.
  bool RefreshAccessToken();

  // Writes the current config_ back to config_path_.
  void SaveConfig() const;

  // Low-level HTTP helpers. Return the response body string.
  // On transport error, return an empty string.
  std::string HttpPost(const std::string& url,
                       const std::string& auth_header,
                       const std::string& content_type,
                       const std::string& post_body,
                       long* http_code_out) const;

  std::string HttpGet(const std::string& url,
                      const std::string& auth_header,
                      long* http_code_out) const;

  VacuumConfig config_;
  std::string config_path_;
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_SMARTTHINGS_CLIENT_HH_
