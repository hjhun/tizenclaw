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

#include "smartthings_client.hh"

#include <curl/curl.h>

#include <fstream>
#include <sstream>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char kStBase[] = "https://api.smartthings.com/v1";
constexpr const char kOauthUrl[] = "https://api.smartthings.com/oauth/token";

// libcurl write callback — appends received data to a std::string.
size_t WriteCallback(void* contents, size_t size, size_t nmemb,
                     std::string* output) {
  size_t total = size * nmemb;
  output->append(static_cast<char*>(contents), total);
  return total;
}

// Minimal JSON string extractor — finds "key":"value" and returns value.
// Handles simple flat JSON objects (no nesting required for config).
std::string JsonExtract(const std::string& json, const std::string& key) {
  std::string needle = "\"" + key + "\"";
  size_t pos = json.find(needle);
  if (pos == std::string::npos) return "";
  pos = json.find(':', pos + needle.size());
  if (pos == std::string::npos) return "";
  pos = json.find('"', pos);
  if (pos == std::string::npos) return "";
  size_t start = pos + 1;
  size_t end = json.find('"', start);
  if (end == std::string::npos) return "";
  return json.substr(start, end - start);
}

// Extracts a numeric JSON value (unquoted integer) for a given key.
std::string JsonExtractNum(const std::string& json, const std::string& key) {
  std::string needle = "\"" + key + "\"";
  size_t pos = json.find(needle);
  if (pos == std::string::npos) return "";
  pos = json.find(':', pos + needle.size());
  if (pos == std::string::npos) return "";
  // Skip whitespace after colon
  while (pos < json.size() && (json[pos] == ':' || json[pos] == ' ')) ++pos;
  size_t start = pos;
  size_t end = start;
  while (end < json.size() && (std::isdigit(json[end]) || json[end] == '-'))
    ++end;
  if (start == end) return "";
  return json.substr(start, end - start);
}

// Reads the full content of a file into a string.
std::string ReadFile(const std::string& path) {
  std::ifstream f(path);
  if (!f.is_open()) return "";
  std::ostringstream ss;
  ss << f.rdbuf();
  return ss.str();
}

// Parses a VacuumConfig from a JSON string.
VacuumConfig ParseConfig(const std::string& json) {
  VacuumConfig cfg;
  cfg.client_id     = JsonExtract(json, "client_id");
  cfg.client_secret = JsonExtract(json, "client_secret");
  cfg.access_token  = JsonExtract(json, "access_token");
  cfg.refresh_token = JsonExtract(json, "refresh_token");
  cfg.device_id     = JsonExtract(json, "device_id");
  return cfg;
}

// Serialises a VacuumConfig back to a JSON string (pretty-printed).
std::string SerialiseConfig(const VacuumConfig& cfg) {
  return "{\n"
         "  \"client_id\": \""     + cfg.client_id     + "\",\n"
         "  \"client_secret\": \"" + cfg.client_secret + "\",\n"
         "  \"access_token\": \""  + cfg.access_token  + "\",\n"
         "  \"refresh_token\": \"" + cfg.refresh_token + "\",\n"
         "  \"device_id\": \""     + cfg.device_id     + "\"\n"
         "}\n";
}

}  // namespace

// ---------------------------------------------------------------------------
// SmartThingsClient
// ---------------------------------------------------------------------------

SmartThingsClient::SmartThingsClient(const std::string& config_path)
    : config_path_(config_path) {
  std::string raw = ReadFile(config_path);
  config_ = ParseConfig(raw);
}

std::string SmartThingsClient::SendCommands(const std::string& body) {
  std::string url =
      std::string(kStBase) + "/devices/" + config_.device_id + "/commands";
  std::string auth = "Authorization: Bearer " + config_.access_token;

  long code = 0;
  HttpPost(url, auth, "application/json", body, &code);

  if (code == 401) {
    if (!RefreshAccessToken())
      return "{\"status\":\"error\",\"message\":\"token refresh failed\"}";
    auth = "Authorization: Bearer " + config_.access_token;
    HttpPost(url, auth, "application/json", body, &code);
  }

  if (code == 204 || code == 200)
    return "{\"status\":\"ok\"}";

  return "{\"status\":\"error\",\"code\":" + std::to_string(code) + "}";
}

std::string SmartThingsClient::GetStatus() {
  std::string url = std::string(kStBase) + "/devices/" + config_.device_id +
                    "/components/main/status";
  std::string auth = "Authorization: Bearer " + config_.access_token;

  long code = 0;
  std::string resp = HttpGet(url, auth, &code);

  if (code == 401) {
    if (!RefreshAccessToken())
      return "{\"status\":\"error\",\"message\":\"token refresh failed\"}";
    auth = "Authorization: Bearer " + config_.access_token;
    resp = HttpGet(url, auth, &code);
  }

  if (code != 200)
    return "{\"status\":\"error\",\"code\":" + std::to_string(code) + "}";

  // Extract the nested attribute values from the SmartThings status payload.
  // The response structure is:
  //   {"robotCleanerMovement":{"robotCleanerMovement":{"value":"charging"}}, ...}
  std::string battery     = JsonExtractNum(resp, "value");
  std::string movement    = JsonExtract(resp, "value");
  std::string clean_mode  = "";
  std::string turbo_mode  = "";

  // Parse each capability block individually for accuracy.
  auto ExtractCapValue = [&](const std::string& cap) -> std::string {
    size_t cap_pos = resp.find("\"" + cap + "\"");
    if (cap_pos == std::string::npos) return "unknown";
    // Find the nested "value" after this capability block.
    size_t val_pos = resp.find("\"value\"", cap_pos);
    if (val_pos == std::string::npos) return "unknown";
    val_pos = resp.find(':', val_pos);
    if (val_pos == std::string::npos) return "unknown";
    // Could be string or number.
    size_t q = resp.find('"', val_pos);
    size_t n = val_pos + 1;
    while (n < resp.size() && resp[n] == ' ') ++n;
    if (q != std::string::npos && (q < n + 5)) {
      // String value
      size_t start = q + 1;
      size_t end   = resp.find('"', start);
      if (end == std::string::npos) return "unknown";
      return resp.substr(start, end - start);
    }
    // Numeric value
    size_t end = n;
    while (end < resp.size() && (std::isdigit(resp[end]) || resp[end] == '-'))
      ++end;
    return resp.substr(n, end - n);
  };

  battery    = ExtractCapValue("battery");
  movement   = ExtractCapValue("robotCleanerMovement");
  clean_mode = ExtractCapValue("robotCleanerCleaningMode");
  turbo_mode = ExtractCapValue("robotCleanerTurboMode");

  return "{\"status\":\"ok\","
         "\"battery_pct\":" + battery + ","
         "\"movement\":\"" + movement + "\","
         "\"cleaning_mode\":\"" + clean_mode + "\","
         "\"turbo_mode\":\"" + turbo_mode + "\"}";
}

bool SmartThingsClient::RefreshAccessToken() {
  std::string userpwd = config_.client_id + ":" + config_.client_secret;
  std::string body = "grant_type=refresh_token"
                     "&refresh_token=" + config_.refresh_token +
                     "&client_id=" + config_.client_id;

  CURL* curl = curl_easy_init();
  if (!curl) return false;

  std::string response_body;
  long http_code = 0;

  curl_easy_setopt(curl, CURLOPT_URL, kOauthUrl);
  curl_easy_setopt(curl, CURLOPT_POST, 1L);
  curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body.c_str());
  curl_easy_setopt(curl, CURLOPT_USERPWD, userpwd.c_str());
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_body);
  curl_easy_setopt(curl, CURLOPT_TIMEOUT, 15L);

  struct curl_slist* headers = nullptr;
  headers = curl_slist_append(
      headers, "Content-Type: application/x-www-form-urlencoded");
  curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);

  curl_easy_perform(curl);
  curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &http_code);
  curl_slist_free_all(headers);
  curl_easy_cleanup(curl);

  if (http_code != 200) return false;

  std::string new_access  = JsonExtract(response_body, "access_token");
  std::string new_refresh = JsonExtract(response_body, "refresh_token");

  if (new_access.empty()) return false;

  config_.access_token  = new_access;
  if (!new_refresh.empty()) config_.refresh_token = new_refresh;
  SaveConfig();
  return true;
}

void SmartThingsClient::SaveConfig() const {
  std::ofstream f(config_path_);
  if (f.is_open()) f << SerialiseConfig(config_);
}

std::string SmartThingsClient::HttpPost(const std::string& url,
                                         const std::string& auth_header,
                                         const std::string& content_type,
                                         const std::string& post_body,
                                         long* http_code_out) const {
  CURL* curl = curl_easy_init();
  if (!curl) { *http_code_out = 0; return ""; }

  std::string response_body;
  struct curl_slist* headers = nullptr;
  headers = curl_slist_append(headers, auth_header.c_str());
  headers = curl_slist_append(
      headers, ("Content-Type: " + content_type).c_str());

  curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
  curl_easy_setopt(curl, CURLOPT_POST, 1L);
  curl_easy_setopt(curl, CURLOPT_POSTFIELDS, post_body.c_str());
  curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE,
                   static_cast<long>(post_body.size()));
  curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_body);
  curl_easy_setopt(curl, CURLOPT_TIMEOUT, 15L);

  curl_easy_perform(curl);
  curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, http_code_out);
  curl_slist_free_all(headers);
  curl_easy_cleanup(curl);
  return response_body;
}

std::string SmartThingsClient::HttpGet(const std::string& url,
                                        const std::string& auth_header,
                                        long* http_code_out) const {
  CURL* curl = curl_easy_init();
  if (!curl) { *http_code_out = 0; return ""; }

  std::string response_body;
  struct curl_slist* headers = nullptr;
  headers = curl_slist_append(headers, auth_header.c_str());

  curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
  curl_easy_setopt(curl, CURLOPT_HTTPGET, 1L);
  curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_body);
  curl_easy_setopt(curl, CURLOPT_TIMEOUT, 15L);

  curl_easy_perform(curl);
  curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, http_code_out);
  curl_slist_free_all(headers);
  curl_easy_cleanup(curl);
  return response_body;
}

}  // namespace cli
}  // namespace tizenclaw
