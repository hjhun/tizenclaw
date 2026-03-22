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
#include "web_dashboard.hh"

#include <cctype>
#include <condition_variable>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <random>
#include <sstream>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"
#include "../scheduler/task_scheduler.hh"
#include "../storage/audit_logger.hh"

namespace tizenclaw {

namespace fs = std::filesystem;

WebDashboard::WebDashboard(AgentCore* agent, TaskScheduler* scheduler)
    : agent_(agent), scheduler_(scheduler) {
  web_root_ = std::string(APP_DATA_DIR) + "/web";
  config_dir_ = std::string(APP_DATA_DIR) + "/config";
  admin_pw_file_ = config_dir_ + "/admin_password.json";
  LoadAdminPassword();

  // Initialize A2A handler
  a2a_handler_ = std::make_unique<A2AHandler>(agent);
  std::string a2a_config = config_dir_ + "/a2a_config.json";
  a2a_handler_->LoadConfig(a2a_config);

  // Initialize health monitor
  health_monitor_ = std::make_unique<HealthMonitor>();
  if (agent_) agent_->SetHealthMonitor(health_monitor_.get());

  // Initialize OTA updater
  std::string skills_dir = std::string(APP_DATA_DIR) + "/tools/skills";
  ota_updater_ = std::make_unique<OtaUpdater>(skills_dir, [this]() {
    if (agent_) agent_->ReloadSkills();
  });
  std::string ota_config = config_dir_ + "/ota_config.json";
  if (!ota_updater_->LoadConfig(ota_config)) {
    LOG(WARNING) << "OTA config not loaded (using defaults)";
  }

  // Initialize tunnel manager
  std::string tunnel_config = config_dir_ + "/tunnel_config.json";
  tunnel_manager_ = std::make_unique<TunnelManager>(tunnel_config);
}

WebDashboard::~WebDashboard() { Stop(); }

bool WebDashboard::LoadConfig() {
  std::string config_path =
      std::string(APP_DATA_DIR) + "/config/dashboard_config.json";
  std::ifstream f(config_path);
  if (f.is_open()) {
    try {
      nlohmann::json j;
      f >> j;
      port_ = j.value("port", 9090);
      web_root_ = j.value("web_root", web_root_);
    } catch (const std::exception& e) {
      LOG(WARNING) << "Failed to parse dashboard " << "config: " << e.what();
    }
  }

  // Check web_root exists
  std::error_code ec;
  if (!fs::is_directory(web_root_, ec)) {
    LOG(WARNING) << "Web root not found: " << web_root_;
    return false;
  }
  return true;
}

void WebDashboard::HandleRequest(SoupServer* /*server*/,
                                 SoupMessage* msg,
                                 const char* path,
                                 GHashTable* query,
                                 SoupClientContext* /*client*/,
                                 gpointer user_data) {
  auto* self = static_cast<WebDashboard*>(user_data);

  // Add CORS headers
  SoupMessageHeaders* resp_headers =
      msg->response_headers;
  soup_message_headers_append(
      resp_headers,
      "Access-Control-Allow-Origin", "*");
  soup_message_headers_append(
      resp_headers,
      "Access-Control-Allow-Methods",
      "GET, POST, OPTIONS");
  soup_message_headers_append(
      resp_headers,
      "Access-Control-Allow-Headers",
      "Content-Type, Authorization");

  // Handle OPTIONS (CORS preflight)
  if (msg->method == SOUP_METHOD_OPTIONS) {
    soup_message_set_status(msg, SOUP_STATUS_OK);
    return;
  }

  std::string req_path(path);

  // A2A: /.well-known/agent.json
  if (req_path == "/.well-known/agent.json") {
    self->ApiAgentCard(msg);
    return;
  }

  // Route API requests
  if (req_path.substr(0, 5) == "/api/") {
    self->HandleApi(msg, req_path, query);
    return;
  }

  // Serve generated web apps from /apps/ path
  if (req_path.substr(0, 6) == "/apps/") {
    self->ServeAppFile(msg, req_path);
    return;
  }

  // Serve SDK files from /sdk/ path
  if (req_path.substr(0, 5) == "/sdk/") {
    self->ServeStaticFile(msg, req_path);
    return;
  }

  // Serve static files
  self->ServeStaticFile(msg, req_path);
}

void WebDashboard::HandleApi(
    SoupMessage* msg, const std::string& path,
    GHashTable* query) {
  if (health_monitor_)
    health_monitor_->IncrementRequestCount();

  if (path == "/api/status") {
    ApiStatus(msg);
  } else if (path == "/api/metrics") {
    ApiMetrics(msg);
  } else if (path == "/api/sessions/dates") {
    ApiSessionDates(msg);
  } else if (path == "/api/sessions") {
    ApiSessions(msg);
  } else if (path.substr(0, 14) ==
             "/api/sessions/") {
    std::string id = path.substr(14);
    ApiSessionDetail(msg, id);
  } else if (path == "/api/tasks/dates") {
    ApiTaskDates(msg);
  } else if (path == "/api/tasks") {
    ApiTasks(msg);
  } else if (path.substr(0, 11) ==
             "/api/tasks/") {
    std::string file = path.substr(11);
    ApiTaskDetail(msg, file);
  } else if (path == "/api/logs/dates") {
    ApiLogDates(msg);
  } else if (path == "/api/logs") {
    // Extract ?date=YYYY-MM-DD query param
    std::string date;
    if (query) {
      const char* dv = static_cast<const char*>(
          g_hash_table_lookup(query, "date"));
      if (dv) date = dv;
    }
    ApiLogs(msg, date);
  } else if (path == "/api/chat") {
    ApiChat(msg);
  } else if (path == "/api/auth/login") {
    const_cast<WebDashboard*>(this)->
        ApiAuthLogin(msg);
  } else if (path ==
             "/api/auth/change_password") {
    const_cast<WebDashboard*>(this)->
        ApiAuthChangePassword(msg);
  } else if (path == "/api/config/list") {
    ApiConfigList(msg);
  } else if (path.substr(0, 12) ==
             "/api/config/") {
    std::string name = path.substr(12);
    if (msg->method == SOUP_METHOD_POST) {
      const_cast<WebDashboard*>(this)->
          ApiConfigSet(msg, name);
    } else {
      ApiConfigGet(msg, name);
    }
  } else if (path == "/api/a2a") {
    const_cast<WebDashboard*>(this)->ApiA2A(msg);
  } else if (path == "/api/ota/check") {
    ApiOtaCheck(msg);
  } else if (path == "/api/ota/update") {
    const_cast<WebDashboard*>(this)->
        ApiOtaUpdate(msg);
  } else if (path == "/api/ota/rollback") {
    const_cast<WebDashboard*>(this)->
        ApiOtaRollback(msg);
  } else if (path == "/api/apps") {
    ApiAppsList(msg);
  } else if (path.substr(0, 10) ==
             "/api/apps/") {
    std::string app_id = path.substr(10);
    if (msg->method == SOUP_METHOD_DELETE) {
      const_cast<WebDashboard*>(this)->
          ApiAppDelete(msg, app_id);
    } else {
      ApiAppDetail(msg, app_id);
    }
  } else if (path == "/api/bridge/tool") {
    const_cast<WebDashboard*>(this)->
        ApiBridgeTool(msg);
  } else if (path == "/api/bridge/tools") {
    ApiBridgeTools(msg, query);
  } else if (path == "/api/bridge/data") {
    const_cast<WebDashboard*>(this)->
        ApiBridgeData(msg, query);
  } else if (path == "/api/bridge/chat") {
    const_cast<WebDashboard*>(this)->
        ApiBridgeChat(msg);
  } else {
    soup_message_set_status(
        msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        "{\"error\":\"Not found\"}", 21);
  }
}

void WebDashboard::ServeStaticFile(SoupMessage* msg,
                                   const std::string& path) const {
  std::string file_path = web_root_;

  if (path == "/" || path.empty()) {
    file_path += "/index.html";
  } else {
    // Prevent directory traversal
    if (path.find("..") != std::string::npos) {
      soup_message_set_status(msg, SOUP_STATUS_FORBIDDEN);
      return;
    }
    file_path += path;
  }

  std::ifstream f(file_path, std::ios::binary);
  if (!f.is_open()) {
    soup_message_set_status(msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(msg, "text/html", SOUP_MEMORY_COPY,
                              "<h1>404 Not Found</h1>", 22);
    return;
  }

  std::string content((std::istreambuf_iterator<char>(f)),
                      std::istreambuf_iterator<char>());

  // Determine MIME type
  std::string content_type = "text/html";
  if (path.size() >= 4) {
    std::string ext = path.substr(path.rfind('.'));
    if (ext == ".css") {
      content_type = "text/css";
    } else if (ext == ".js") {
      content_type = "application/javascript";
    } else if (ext == ".json") {
      content_type = "application/json";
    } else if (ext == ".png") {
      content_type = "image/png";
    } else if (ext == ".svg") {
      content_type = "image/svg+xml";
    } else if (ext == ".jpg" || ext == ".jpeg") {
      content_type = "image/jpeg";
    }
  }

  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, content_type.c_str(), SOUP_MEMORY_COPY,
                            content.c_str(),
                            static_cast<gsize>(content.size()));
}

void WebDashboard::ApiStatus(SoupMessage* msg) const {
  nlohmann::json status = {{"status", "running"},
                           {"version", "1.0.0"},
                           {"channels", agent_ ? "active" : "inactive"}};

  if (tunnel_manager_ && tunnel_manager_->IsRunning()) {
    std::string url = tunnel_manager_->GetPublicUrl();
    if (!url.empty()) {
      status["tunnel_url"] = url;
    }
  }
  std::string body = status.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiMetrics(SoupMessage* msg) const {
  if (!health_monitor_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  nlohmann::json metrics =
      nlohmann::json::parse(health_monitor_->GetMetricsJson());
  metrics["version"] = "1.0.0";
  metrics["status"] = "running";

  std::string body = metrics.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiOtaCheck(SoupMessage* msg) const {
  if (!ota_updater_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  std::string body = ota_updater_->CheckForUpdates();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiOtaUpdate(SoupMessage* msg) {
  if (!ota_updater_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  // Parse skill name from POST body
  SoupMessageBody* req_body = msg->request_body;
  std::string req_str(req_body->data, req_body->length);
  std::string skill_name;
  try {
    auto j = nlohmann::json::parse(req_str);
    skill_name = j.value("skill", "");
  } catch (...) {
  }

  if (skill_name.empty()) {
    std::string err = "{\"error\":\"skill name required\"}";
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              err.c_str(), static_cast<gsize>(err.size()));
    return;
  }

  std::string body = ota_updater_->UpdateSkill(skill_name);
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiOtaRollback(SoupMessage* msg) {
  if (!ota_updater_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  SoupMessageBody* req_body = msg->request_body;
  std::string req_str(req_body->data, req_body->length);
  std::string skill_name;
  try {
    auto j = nlohmann::json::parse(req_str);
    skill_name = j.value("skill", "");
  } catch (...) {
  }

  if (skill_name.empty()) {
    std::string err = "{\"error\":\"skill name required\"}";
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              err.c_str(), static_cast<gsize>(err.size()));
    return;
  }

  std::string body = ota_updater_->RollbackSkill(skill_name);
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

namespace {

// Convert fs::file_time_type to epoch seconds.
int64_t FileTimeToEpoch(
    const fs::file_time_type& ft) {
  auto sctp = std::chrono::time_point_cast<
      std::chrono::seconds>(
      ft - fs::file_time_type::clock::now() +
      std::chrono::system_clock::now());
  return sctp.time_since_epoch().count();
}

// Convert epoch seconds to "YYYY-MM-DD" string.
std::string EpochToDateStr(int64_t epoch) {
  auto t = static_cast<std::time_t>(epoch);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%d");
  return oss.str();
}

// Get today's date as "YYYY-MM-DD".
std::string TodayDateStr() {
  auto now = std::chrono::system_clock::now();
  auto t =
      std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%d");
  return oss.str();
}

struct ChatCtx {
  WebDashboard* self;
  SoupMessage* msg;
  std::string result;
  std::string session_id;
  std::string prompt;
};

}  // namespace

void WebDashboard::ApiSessions(
    SoupMessage* msg) const {
  nlohmann::json sessions =
      nlohmann::json::array();

  fs::path sessions_dir =
      std::string(APP_DATA_DIR) + "/sessions";
  std::error_code ec;
  for (const auto& entry :
       fs::directory_iterator(sessions_dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name =
        entry.path().filename().string();
    if (name.empty() || name[0] == '.') continue;
    if (name.size() <= 3 ||
        name.substr(name.size() - 3) != ".md")
      continue;

    std::string id =
        name.substr(0, name.size() - 3);

    nlohmann::json entry_j;
    entry_j["id"] = id;
    entry_j["file"] = name;
    std::error_code fec;
    entry_j["size_bytes"] =
        static_cast<int64_t>(entry.file_size(fec));

    auto lwt = entry.last_write_time(fec);
    int64_t mod = FileTimeToEpoch(lwt);
    entry_j["modified"] = mod;
    entry_j["date"] = EpochToDateStr(mod);
    sessions.push_back(std::move(entry_j));
  }

  std::string body = sessions.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiSessionDates(
    SoupMessage* msg) const {
  std::set<std::string> dates;
  fs::path sessions_dir =
      std::string(APP_DATA_DIR) + "/sessions";
  std::error_code ec;
  for (const auto& entry :
       fs::directory_iterator(sessions_dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name =
        entry.path().filename().string();
    if (name.empty() || name[0] == '.') continue;
    if (name.size() <= 3 ||
        name.substr(name.size() - 3) != ".md")
      continue;

    std::error_code fec;
    auto lwt = entry.last_write_time(fec);
    dates.insert(
        EpochToDateStr(FileTimeToEpoch(lwt)));
  }

  nlohmann::json arr = nlohmann::json::array();
  for (const auto& d : dates) arr.push_back(d);

  nlohmann::json resp = {{"dates", arr}};
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiSessionDetail(SoupMessage* msg,
                                    const std::string& id) const {
  // Prevent path traversal
  if (id.empty() || id.find("..") != std::string::npos ||
      id.find('/') != std::string::npos) {
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Invalid id\"}", 21);
    return;
  }

  fs::path file_path = std::string(APP_DATA_DIR) + "/sessions/" + id + ".md";
  std::ifstream f(file_path);
  if (!f.is_open()) {
    soup_message_set_status(msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Session not found\"}", 29);
    return;
  }

  std::string content((std::istreambuf_iterator<char>(f)),
                      std::istreambuf_iterator<char>());

  nlohmann::json resp;
  resp["id"] = id;
  resp["content"] = std::move(content);
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiTasks(
    SoupMessage* msg) const {
  nlohmann::json tasks = nlohmann::json::array();

  fs::path tasks_dir =
      std::string(APP_DATA_DIR) + "/tasks";
  std::error_code ec;
  for (const auto& entry :
       fs::directory_iterator(tasks_dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name =
        entry.path().filename().string();
    if (name.empty() || name[0] == '.') continue;
    if (name.size() <= 3 ||
        name.substr(name.size() - 3) != ".md")
      continue;

    // Read task file for metadata (only first 256 bytes)
    std::ifstream tf(entry.path());
    std::string content_preview;
    if (tf.is_open()) {
      char buf[256] = {0};
      tf.read(buf, sizeof(buf) - 1);
      content_preview.assign(buf, tf.gcount());
    }

    nlohmann::json task_j;
    task_j["file"] = name;
    task_j["content_preview"] = content_preview;

    std::error_code fec;
    auto lwt = entry.last_write_time(fec);
    int64_t mod = FileTimeToEpoch(lwt);
    task_j["modified"] = mod;
    task_j["date"] = EpochToDateStr(mod);
    tasks.push_back(std::move(task_j));
  }

  std::string body = tasks.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiTaskDates(
    SoupMessage* msg) const {
  std::set<std::string> dates;
  fs::path tasks_dir =
      std::string(APP_DATA_DIR) + "/tasks";
  std::error_code ec;
  for (const auto& entry :
       fs::directory_iterator(tasks_dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name =
        entry.path().filename().string();
    if (name.empty() || name[0] == '.') continue;
    if (name.size() <= 3 ||
        name.substr(name.size() - 3) != ".md")
      continue;

    std::error_code fec;
    auto lwt = entry.last_write_time(fec);
    dates.insert(
        EpochToDateStr(FileTimeToEpoch(lwt)));
  }

  nlohmann::json arr = nlohmann::json::array();
  for (const auto& d : dates) arr.push_back(d);

  nlohmann::json resp = {{"dates", arr}};
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiTaskDetail(SoupMessage* msg,
                                 const std::string& file) const {
  // Prevent path traversal
  if (file.empty() || file.find("..") != std::string::npos ||
      file.find('/') != std::string::npos) {
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Invalid file\"}", 23);
    return;
  }

  // Enforce .md extension
  std::string fname = file;
  if (fname.size() <= 3 || fname.substr(fname.size() - 3) != ".md") {
    fname += ".md";
  }

  fs::path file_path = std::string(APP_DATA_DIR) + "/tasks/" + fname;
  std::ifstream f(file_path);
  if (!f.is_open()) {
    soup_message_set_status(msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Task not found\"}", 26);
    return;
  }

  std::string content((std::istreambuf_iterator<char>(f)),
                      std::istreambuf_iterator<char>());

  nlohmann::json resp;
  resp["file"] = fname;
  resp["content"] = std::move(content);
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiLogs(
    SoupMessage* msg,
    const std::string& date_param) const {
  nlohmann::json logs = nlohmann::json::array();

  // Use provided date or default to today
  std::string date =
      date_param.empty() ? TodayDateStr()
                         : date_param;

  // Validate date format (YYYY-MM-DD)
  if (date.size() != 10 || date[4] != '-' ||
      date[7] != '-') {
    std::string err =
        "{\"error\":\"Invalid date format\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY, err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  std::string log_path =
      std::string(APP_DATA_DIR) + "/audit/" +
      date + ".md";
  std::ifstream lf(log_path);
  if (lf.is_open()) {
    std::string content(
        (std::istreambuf_iterator<char>(lf)),
        std::istreambuf_iterator<char>());
    logs.push_back(
        {{"date", date},
         {"content", std::move(content)}});
  }

  std::string body = logs.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiLogDates(
    SoupMessage* msg) const {
  std::set<std::string> dates;
  fs::path audit_dir =
      std::string(APP_DATA_DIR) + "/audit";
  std::error_code ec;
  for (const auto& entry :
       fs::directory_iterator(audit_dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name =
        entry.path().filename().string();
    // Match YYYY-MM-DD.md pattern
    if (name.size() == 13 &&
        name.substr(10) == ".md" &&
        name[4] == '-' && name[7] == '-') {
      dates.insert(name.substr(0, 10));
    }
  }

  nlohmann::json arr = nlohmann::json::array();
  for (const auto& d : dates) arr.push_back(d);

  nlohmann::json resp = {{"dates", arr}};
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json", SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiChat(SoupMessage* msg) {
  // Only accept POST
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  // Extract body
  SoupMessageBody* body = msg->request_body;
  std::string payload;
  if (body && body->data && body->length > 0) {
    payload.assign(body->data, body->length);
  }

  std::string prompt;
  std::string session_id = "web_dashboard";
  try {
    auto j = nlohmann::json::parse(payload);
    prompt = j.value("prompt", "");
    session_id = j.value("session_id", "web_dashboard");
  } catch (...) {
    prompt = payload;
  }

  if (prompt.empty() || !agent_) {
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Empty prompt\"}", 24);
    return;
  }

  // Reject new requests during shutdown to prevent Stop() deadlock.
  if (!running_.load()) {
    soup_message_set_status(msg, SOUP_STATUS_SERVICE_UNAVAILABLE);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Shutting down\"}", 24);
    return;
  }

  // Pause the HTTP response so the GMainLoop thread is not blocked.
  // ProcessPrompt runs on a worker thread; once it finishes, the
  // result is dispatched back via g_main_context_invoke and the message is
  // unpaused, allowing the response to be sent.
  g_object_ref(msg);  // prevent libsoup from freeing msg early
  soup_server_pause_message(server_, msg);
  auto* ctx = new ChatCtx{this, msg, "", session_id, prompt};

  pending_workers_.fetch_add(1);
  if (chat_thread_pool_) {
    g_thread_pool_push(chat_thread_pool_, ctx, nullptr);
  } else {
    // Fallback if pool creation failed
    std::thread([ctx]() {
      ctx->result = ctx->self->agent_->ProcessPrompt(ctx->session_id, ctx->prompt);
      g_main_context_invoke(ctx->self->context_, [](gpointer data) -> gboolean {
        auto* c = static_cast<ChatCtx*>(data);
        nlohmann::json resp = {{"status", "ok"}, {"session_id", c->session_id}, {"response", c->result}};
        std::string resp_str = resp.dump();
        soup_message_set_status(c->msg, SOUP_STATUS_OK);
        soup_message_set_response(c->msg, "application/json", SOUP_MEMORY_COPY, resp_str.c_str(), static_cast<gsize>(resp_str.size()));
        soup_server_unpause_message(c->self->server_, c->msg);
        g_object_unref(c->msg);
        auto* self = c->self;
        delete c;
        self->pending_workers_.fetch_sub(1);
        std::lock_guard<std::mutex> lk(self->workers_mutex_);
        self->workers_cv_.notify_all();
        return G_SOURCE_REMOVE;
      }, ctx);
    }).detach();
  }
}

bool WebDashboard::Start() {
  if (running_) return true;

  if (!LoadConfig()) {
    LOG(WARNING) << "WebDashboard: no web root, " << "skipping";
    return false;
  }

  // Create a dedicated GMainContext so that SoupServer and
  // g_main_context_invoke() callbacks all run on the server thread,
  context_ = g_main_context_new();

  // Initialize Chat Thread Pool limit (2 concurent chats MAX)
  chat_thread_pool_ = g_thread_pool_new(
      [](gpointer data, gpointer /* user_data */) {
        auto* ctx = static_cast<ChatCtx*>(data);
        ctx->result = ctx->self->agent_->ProcessPrompt(ctx->session_id, ctx->prompt);
        g_main_context_invoke(ctx->self->context_, [](gpointer data2) -> gboolean {
          auto* c = static_cast<ChatCtx*>(data2);
          nlohmann::json resp = {{"status", "ok"}, {"session_id", c->session_id}, {"response", c->result}};
          std::string resp_str = resp.dump();
          soup_message_set_status(c->msg, SOUP_STATUS_OK);
          soup_message_set_response(c->msg, "application/json", SOUP_MEMORY_COPY, resp_str.c_str(), static_cast<gsize>(resp_str.size()));
          soup_server_unpause_message(c->self->server_, c->msg);
          g_object_unref(c->msg);
          auto* self = c->self;
          delete c;
          self->pending_workers_.fetch_sub(1);
          std::lock_guard<std::mutex> lk(self->workers_mutex_);
          self->workers_cv_.notify_all();
          return G_SOURCE_REMOVE;
        }, ctx);
      },
      this, 2, FALSE, nullptr);

  // SoupServer must be created and start listening inside the
  // server thread so that libsoup attaches its internal sources
  // to the dedicated context (thread-default at that point).
  std::mutex mtx;
  std::condition_variable cv;
  bool ready = false;
  bool ok = false;

  server_thread_ = std::thread([this, &mtx, &cv, &ready, &ok]() {
    g_main_context_push_thread_default(context_);

    server_ = soup_server_new(
        SOUP_SERVER_SERVER_HEADER, "TizenClaw-Dashboard", nullptr);
    if (!server_) {
      LOG(ERROR) << "Failed to create " << "dashboard SoupServer";
      std::lock_guard<std::mutex> lk(mtx);
      ok = false;
      ready = true;
      cv.notify_one();
      g_main_context_pop_thread_default(context_);
      return;
    }

    soup_server_add_handler(server_, "/", HandleRequest, this, nullptr);

    GError* error = nullptr;
    if (!soup_server_listen_all(
            server_, port_,
            static_cast<SoupServerListenOptions>(0), &error)) {
      LOG(ERROR) << "Dashboard: failed to listen "
                 << "on port " << port_ << ": "
                 << error->message;
      g_error_free(error);
      g_object_unref(server_);
      server_ = nullptr;
      std::lock_guard<std::mutex> lk(mtx);
      ok = false;
      ready = true;
      cv.notify_one();
      g_main_context_pop_thread_default(context_);
      return;
    }

    loop_ = g_main_loop_new(context_, FALSE);

    {
      std::lock_guard<std::mutex> lk(mtx);
      ok = true;
      ready = true;
      cv.notify_one();
    }

    LOG(INFO) << "Web dashboard running on " << "port " << port_;

    if (tunnel_manager_) {
      tunnel_manager_->StartTunnel(port_);
    }

    g_main_loop_run(loop_);

    if (tunnel_manager_) {
      tunnel_manager_->StopTunnel();
    }
    g_main_loop_unref(loop_);
    loop_ = nullptr;
    g_main_context_pop_thread_default(context_);
  });

  // Wait for the server thread to finish initialization
  {
    std::unique_lock<std::mutex> lk(mtx);
    cv.wait(lk, [&ready]() { return ready; });
  }

  if (!ok) {
    if (server_thread_.joinable()) server_thread_.join();
    g_main_context_unref(context_);
    context_ = nullptr;
    return false;
  }

  running_ = true;
  LOG(INFO) << "WebDashboard started on " << "port " << port_;
  return true;
}

void WebDashboard::Stop() {
  if (!running_) return;

  running_ = false;

  // Wait for all pending worker threads to finish
  // before destroying server/context resources.
  {
    std::unique_lock<std::mutex> lk(workers_mutex_);
    workers_cv_.wait(lk, [this]() {
      return pending_workers_.load() == 0;
    });
  }

  if (loop_) {
    g_main_loop_quit(loop_);
  }

  if (server_thread_.joinable()) {
    server_thread_.join();
  }

  if (server_) {
    soup_server_disconnect(server_);
    g_object_unref(server_);
    server_ = nullptr;
  }

  if (context_) {
    g_main_context_unref(context_);
    context_ = nullptr;
  }

  if (chat_thread_pool_) {
    g_thread_pool_free(chat_thread_pool_, FALSE, TRUE);
    chat_thread_pool_ = nullptr;
  }

  LOG(INFO) << "WebDashboard stopped.";
}

void WebDashboard::ApiAgentCard(SoupMessage* msg) const {
  if (!a2a_handler_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  nlohmann::json card = a2a_handler_->GetAgentCard();
  std::string body = card.dump(2);
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

void WebDashboard::ApiA2A(SoupMessage* msg) {
  // Only accept POST for JSON-RPC
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  if (!a2a_handler_) {
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    return;
  }

  // Validate Bearer token
  const char* auth =
      soup_message_headers_get_one(msg->request_headers, "Authorization");
  std::string token;
  if (auth) {
    std::string hdr(auth);
    if (hdr.size() > 7 && hdr.substr(0, 7) == "Bearer ") {
      token = hdr.substr(7);
    }
  }

  if (!a2a_handler_->ValidateBearerToken(token)) {
    nlohmann::json err = {
        {"jsonrpc", "2.0"},
        {"id", nullptr},
        {"error", {{"code", -32000}, {"message", "Unauthorized"}}}};
    std::string body = err.dump();
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              body.c_str(), static_cast<gsize>(body.size()));
    return;
  }

  // Parse JSON-RPC request body
  SoupMessageBody* req_body = msg->request_body;
  std::string payload;
  if (req_body && req_body->data && req_body->length > 0) {
    payload.assign(req_body->data, req_body->length);
  }

  nlohmann::json request;
  try {
    request = nlohmann::json::parse(payload);
  } catch (...) {
    nlohmann::json err = {
        {"jsonrpc", "2.0"},
        {"id", nullptr},
        {"error", {{"code", -32700}, {"message", "Parse error"}}}};
    std::string body = err.dump();
    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              body.c_str(), static_cast<gsize>(body.size()));
    return;
  }

  // Handle JSON-RPC
  nlohmann::json response = a2a_handler_->HandleJsonRpc(request);

  std::string body = response.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            body.c_str(), static_cast<gsize>(body.size()));
}

}  // namespace tizenclaw

// Auth and Config implementations below
namespace tizenclaw {

const std::vector<std::string> WebDashboard::kAllowedConfigs = {
    "llm_config.json",     "telegram_config.json", "slack_config.json",
    "discord_config.json", "webhook_config.json",  "tool_policy.json",
    "agent_roles.json",    "tunnel_config.json",   "web_search_config.json"};

// --- Auth helpers ---

std::string WebDashboard::HashPassword(const std::string& pw) const {
  gchar* checksum = g_compute_checksum_for_string(
      G_CHECKSUM_SHA256, pw.c_str(), static_cast<gssize>(pw.size()));
  std::string result(checksum);
  g_free(checksum);
  return result;
}

std::string WebDashboard::GenerateToken() const {
  static constexpr char kHexChars[] = "0123456789abcdef";
  std::random_device rd;
  std::mt19937 gen(rd());
  std::uniform_int_distribution<int> dist(0, 15);
  std::string token;
  token.reserve(32);
  for (int i = 0; i < 32; ++i) {
    token += kHexChars[dist(gen)];
  }
  return token;
}

void WebDashboard::LoadAdminPassword() {
  // Default: sha256("admin")
  admin_password_hash_ = HashPassword("admin");

  std::ifstream f(admin_pw_file_);
  if (f.is_open()) {
    try {
      nlohmann::json j;
      f >> j;
      admin_password_hash_ = j.value("password_hash", admin_password_hash_);
    } catch (...) {
      LOG(WARNING) << "Failed to load admin password";
    }
  }
}

void WebDashboard::SaveAdminPassword() {
  nlohmann::json j = {{"password_hash", admin_password_hash_}};
  std::ofstream f(admin_pw_file_);
  if (f.is_open()) {
    f << j.dump(2);
  }
}

bool WebDashboard::ValidateToken(SoupMessage* msg) const {
  const char* auth =
      soup_message_headers_get_one(msg->request_headers, "Authorization");
  if (!auth) return false;

  std::string hdr(auth);
  if (hdr.substr(0, 7) != "Bearer ") return false;

  std::string token = hdr.substr(7);
  std::lock_guard<std::mutex> lock(tokens_mutex_);
  return active_tokens_.count(token) > 0;
}

void WebDashboard::ApiAuthLogin(SoupMessage* msg) {
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  SoupMessageBody* body = msg->request_body;
  std::string payload;
  if (body && body->data && body->length > 0)
    payload.assign(body->data, body->length);

  std::string password;
  try {
    auto j = nlohmann::json::parse(payload);
    password = j.value("password", "");
  } catch (...) {
    password = payload;
  }

  if (HashPassword(password) == admin_password_hash_) {
    std::string token = GenerateToken();
    {
      std::lock_guard<std::mutex> lock(tokens_mutex_);
      active_tokens_.insert(token);
    }

    nlohmann::json resp = {{"status", "ok"}, {"token", token}};
    std::string r = resp.dump();
    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              r.c_str(), static_cast<gsize>(r.size()));
    LOG(INFO) << "Admin login successful";
  } else {
    nlohmann::json resp = {{"status", "error"}, {"error", "Invalid password"}};
    std::string r = resp.dump();
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              r.c_str(), static_cast<gsize>(r.size()));
    LOG(WARNING) << "Admin login failed";
  }
}

void WebDashboard::ApiAuthChangePassword(SoupMessage* msg) {
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  if (!ValidateToken(msg)) {
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Unauthorized\"}", 24);
    return;
  }

  SoupMessageBody* body = msg->request_body;
  std::string payload;
  if (body && body->data && body->length > 0)
    payload.assign(body->data, body->length);

  try {
    auto j = nlohmann::json::parse(payload);
    std::string cur = j.value("current_password", "");
    std::string nw = j.value("new_password", "");

    if (HashPassword(cur) != admin_password_hash_) {
      nlohmann::json r = {{"status", "error"},
                          {"error", "Current password incorrect"}};
      std::string s = r.dump();
      soup_message_set_status(msg, SOUP_STATUS_FORBIDDEN);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                s.c_str(), static_cast<gsize>(s.size()));
      return;
    }

    if (nw.empty()) {
      nlohmann::json r = {{"status", "error"}, {"error", "New password empty"}};
      std::string s = r.dump();
      soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                s.c_str(), static_cast<gsize>(s.size()));
      return;
    }

    admin_password_hash_ = HashPassword(nw);
    SaveAdminPassword();

    nlohmann::json r = {{"status", "ok"}};
    std::string s = r.dump();
    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              s.c_str(), static_cast<gsize>(s.size()));
    LOG(INFO) << "Admin password changed";
  } catch (...) {
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
  }
}

// --- Config helpers ---

bool WebDashboard::IsAllowedConfig(const std::string& name) const {
  for (const auto& c : kAllowedConfigs) {
    if (c == name) return true;
  }
  return false;
}

std::string WebDashboard::ConfigFilePath(const std::string& name) const {
  return config_dir_ + "/" + name;
}

std::string WebDashboard::SampleFilePath(const std::string& name) const {
  return config_dir_ + "/" + name + ".sample";
}

void WebDashboard::ApiConfigList(SoupMessage* msg) const {
  if (!ValidateToken(msg)) {
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Unauthorized\"}", 24);
    return;
  }

  nlohmann::json configs = nlohmann::json::array();
  for (const auto& name : kAllowedConfigs) {
    std::string fpath = ConfigFilePath(name);
    bool exists = fs::exists(fpath);
    configs.push_back({{"name", name}, {"exists", exists}});
  }

  nlohmann::json resp = {{"status", "ok"}, {"configs", configs}};
  std::string r = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            r.c_str(), static_cast<gsize>(r.size()));
}

void WebDashboard::ApiConfigGet(SoupMessage* msg,
                                const std::string& name) const {
  if (!ValidateToken(msg)) {
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Unauthorized\"}", 24);
    return;
  }

  if (!IsAllowedConfig(name)) {
    soup_message_set_status(msg, SOUP_STATUS_FORBIDDEN);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Not allowed\"}", 23);
    return;
  }

  std::string fpath = ConfigFilePath(name);
  std::ifstream f(fpath);
  if (f.is_open()) {
    std::string content((std::istreambuf_iterator<char>(f)),
                        std::istreambuf_iterator<char>());
    nlohmann::json resp = {
        {"status", "ok"}, {"name", name}, {"content", content}};
    std::string r = resp.dump();
    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              r.c_str(), static_cast<gsize>(r.size()));
  } else {
    // Try sample file
    std::string sample_path = SampleFilePath(name);
    std::ifstream sf(sample_path);
    std::string sample_content;
    if (sf.is_open()) {
      sample_content.assign((std::istreambuf_iterator<char>(sf)),
                            std::istreambuf_iterator<char>());
    }
    nlohmann::json resp = {{"status", "not_found"},
                           {"name", name},
                           {"error", "Config not found"},
                           {"sample", sample_content}};
    std::string r = resp.dump();
    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              r.c_str(), static_cast<gsize>(r.size()));
  }
}

void WebDashboard::ApiConfigSet(SoupMessage* msg, const std::string& name) {
  if (!ValidateToken(msg)) {
    soup_message_set_status(msg, SOUP_STATUS_UNAUTHORIZED);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Unauthorized\"}", 24);
    return;
  }

  if (!IsAllowedConfig(name)) {
    soup_message_set_status(msg, SOUP_STATUS_FORBIDDEN);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Not allowed\"}", 23);
    return;
  }

  SoupMessageBody* body = msg->request_body;
  std::string payload;
  if (body && body->data && body->length > 0)
    payload.assign(body->data, body->length);

  std::string content;
  try {
    auto j = nlohmann::json::parse(payload);
    content = j.value("content", "");
  } catch (...) {
    soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
    return;
  }

  std::string fpath = ConfigFilePath(name);

  // Backup existing file
  if (fs::exists(fpath)) {
    std::string backup = fpath + ".bak";
    std::error_code bec;
    fs::copy_file(fpath, backup, fs::copy_options::overwrite_existing, bec);
  }

  // Write new content
  std::ofstream out(fpath);
  if (!out.is_open()) {
    nlohmann::json resp = {{"status", "error"},
                           {"error", "Failed to write config"}};
    std::string r = resp.dump();
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              r.c_str(), static_cast<gsize>(r.size()));
    return;
  }

  out << content;
  out.close();

  LOG(INFO) << "Config saved: " << name;

  nlohmann::json resp = {{"status", "ok"}, {"name", name}};
  std::string r = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                            r.c_str(), static_cast<gsize>(r.size()));
}

void WebDashboard::ApiAppsList(
    SoupMessage* msg) const {
  nlohmann::json apps = nlohmann::json::array();

  fs::path apps_dir =
      std::string(APP_DATA_DIR) + "/web/apps";
  std::error_code ec;
  if (!fs::is_directory(apps_dir, ec)) {
    std::string body = apps.dump();
    soup_message_set_status(
        msg, SOUP_STATUS_OK);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        body.c_str(),
        static_cast<gsize>(body.size()));
    return;
  }

  for (const auto& entry :
       fs::directory_iterator(apps_dir, ec)) {
    if (!entry.is_directory(ec)) continue;
    std::string dirname =
        entry.path().filename().string();
    if (dirname.empty() || dirname[0] == '.')
      continue;

    nlohmann::json app_j;
    app_j["app_id"] = dirname;
    app_j["url"] = "/apps/" + dirname + "/";

    // Read manifest.json if exists
    std::string manifest_path =
        entry.path().string() +
        "/manifest.json";
    std::ifstream mf(manifest_path);
    if (mf.is_open()) {
      try {
        nlohmann::json manifest;
        mf >> manifest;
        app_j["title"] =
            manifest.value("title", dirname);
        app_j["created_at"] =
            manifest.value("created_at", 0);
        app_j["has_css"] =
            manifest.value("has_css", false);
        app_j["has_js"] =
            manifest.value("has_js", false);
      } catch (...) {
        app_j["title"] = dirname;
      }
    } else {
      app_j["title"] = dirname;
    }

    apps.push_back(std::move(app_j));
  }

  std::string body = apps.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json",
      SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiAppDetail(
    SoupMessage* msg,
    const std::string& app_id) const {
  // Prevent path traversal
  if (app_id.empty() ||
      app_id.find("..") != std::string::npos ||
      app_id.find('/') != std::string::npos) {
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        "{\"error\":\"Invalid app_id\"}", 25);
    return;
  }

  fs::path app_dir =
      std::string(APP_DATA_DIR) +
      "/web/apps/" + app_id;
  std::error_code ec;
  if (!fs::is_directory(app_dir, ec)) {
    soup_message_set_status(
        msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        "{\"error\":\"App not found\"}", 24);
    return;
  }

  nlohmann::json app_j;
  app_j["app_id"] = app_id;
  app_j["url"] = "/apps/" + app_id + "/";

  // Read manifest
  std::string manifest_path =
      app_dir.string() + "/manifest.json";
  std::ifstream mf(manifest_path);
  if (mf.is_open()) {
    try {
      nlohmann::json manifest;
      mf >> manifest;
      app_j.merge_patch(manifest);
    } catch (...) {}
  }

  // List files
  nlohmann::json files =
      nlohmann::json::array();
  for (const auto& f :
       fs::directory_iterator(app_dir, ec)) {
    if (!f.is_regular_file(ec)) continue;
    files.push_back(
        f.path().filename().string());
  }
  app_j["files"] = files;

  std::string body = app_j.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json",
      SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiAppDelete(
    SoupMessage* msg,
    const std::string& app_id) {
  // Prevent path traversal
  if (app_id.empty() ||
      app_id.find("..") != std::string::npos ||
      app_id.find('/') != std::string::npos) {
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        "{\"error\":\"Invalid app_id\"}", 25);
    return;
  }

  fs::path app_dir =
      std::string(APP_DATA_DIR) +
      "/web/apps/" + app_id;
  std::error_code ec;
  if (!fs::is_directory(app_dir, ec)) {
    soup_message_set_status(
        msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        "{\"error\":\"App not found\"}", 24);
    return;
  }

  fs::remove_all(app_dir, ec);
  if (ec) {
    std::string err =
        "{\"error\":\"Delete failed: " +
        ec.message() + "\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  LOG(INFO) << "WebDashboard: deleted app '"
            << app_id << "'";

  nlohmann::json resp = {
      {"status", "deleted"},
      {"app_id", app_id}};
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json",
      SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ServeAppFile(
    SoupMessage* msg,
    const std::string& path) const {
  // path format: /apps/{app_id}/[file]
  // Extract app_id and file parts
  std::string rel = path.substr(6);  // remove "/apps/"

  // Prevent directory traversal
  if (rel.find("..") != std::string::npos) {
    soup_message_set_status(
        msg, SOUP_STATUS_FORBIDDEN);
    return;
  }

  std::string file_path =
      std::string(APP_DATA_DIR) +
      "/web/apps/" + rel;

  // If path ends with /, serve index.html
  if (rel.empty() || rel.back() == '/') {
    file_path += "index.html";
  }

  std::ifstream f(file_path, std::ios::binary);
  if (!f.is_open()) {
    soup_message_set_status(
        msg, SOUP_STATUS_NOT_FOUND);
    soup_message_set_response(
        msg, "text/html", SOUP_MEMORY_COPY,
        "<h1>404 Not Found</h1>", 22);
    return;
  }

  std::string content(
      (std::istreambuf_iterator<char>(f)),
      std::istreambuf_iterator<char>());

  // Determine MIME type
  std::string content_type = "text/html";
  auto dot_pos = file_path.rfind('.');
  if (dot_pos != std::string::npos) {
    std::string ext =
        file_path.substr(dot_pos);
    if (ext == ".css") {
      content_type = "text/css";
    } else if (ext == ".js") {
      content_type = "application/javascript";
    } else if (ext == ".json") {
      content_type = "application/json";
    } else if (ext == ".png") {
      content_type = "image/png";
    } else if (ext == ".svg") {
      content_type = "image/svg+xml";
    } else if (ext == ".jpg" ||
               ext == ".jpeg") {
      content_type = "image/jpeg";
    } else if (ext == ".gif") {
      content_type = "image/gif";
    } else if (ext == ".webp") {
      content_type = "image/webp";
    } else if (ext == ".ico") {
      content_type = "image/x-icon";
    } else if (ext == ".woff" ||
               ext == ".woff2") {
      content_type = "font/woff2";
    }
  }

  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, content_type.c_str(),
      SOUP_MEMORY_COPY,
      content.c_str(),
      static_cast<gsize>(content.size()));
}

void WebDashboard::ApiBridgeTool(
    SoupMessage* msg) {
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(
        msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  SoupMessageBody* req_body = msg->request_body;
  if (!req_body || !req_body->data ||
      req_body->length == 0) {
    std::string err =
        "{\"error\":\"Empty request body\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  std::string payload(
      req_body->data, req_body->length);

  std::string app_id;
  std::string tool_name;
  nlohmann::json tool_args =
      nlohmann::json::object();

  try {
    auto j = nlohmann::json::parse(payload);
    app_id = j.value("app_id", "");
    tool_name = j.value("tool_name", "");
    if (j.contains("arguments") &&
        j["arguments"].is_object()) {
      tool_args = j["arguments"];
    }
  } catch (const std::exception& ex) {
    std::string err =
        "{\"error\":\"Invalid JSON: " +
        std::string(ex.what()) + "\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  if (app_id.empty() || tool_name.empty()) {
    std::string err =
        "{\"error\":\"app_id and tool_name "
        "are required\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  if (!agent_) {
    std::string err =
        "{\"error\":\"Agent not available\"}";
    soup_message_set_status(
        msg,
        SOUP_STATUS_INTERNAL_SERVER_ERROR);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  // Load allowed tools from app manifest
  auto allowed = LoadAppAllowedTools(app_id);

  // Rate limiting check
  if (!CheckBridgeRateLimit(app_id)) {
    std::string err =
        "{\"error\":\"Rate limit exceeded\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_FORBIDDEN);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  // Dispatch tool via AgentCore
  std::string result =
      agent_->ExecuteBridgeTool(
          tool_name, tool_args, allowed);

  // Audit log the bridge call
  AuditLogger::Instance().Log(
      AuditLogger::MakeEvent(
          AuditEventType::kToolExecution,
          "bridge:" + app_id,
          {{"tool", tool_name},
           {"source", "bridge_api"}}));

  nlohmann::json resp;
  try {
    resp = nlohmann::json::parse(result);
  } catch (...) {
    resp = {{"result", result}};
  }

  // Wrap in status envelope
  nlohmann::json envelope;
  if (resp.contains("error")) {
    envelope = {{"status", "error"},
                {"error", resp["error"]}};
  } else {
    envelope = {{"status", "ok"},
                {"result", resp}};
  }

  std::string body = envelope.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json",
      SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

void WebDashboard::ApiBridgeTools(
    SoupMessage* msg,
    GHashTable* query) const {
  std::string app_id;
  if (query) {
    const char* aid =
        static_cast<const char*>(
            g_hash_table_lookup(
                query, "app_id"));
    if (aid) app_id = aid;
  }

  // Load allowed_tools from manifest
  auto allowed = LoadAppAllowedTools(app_id);

  // Get all tool declarations
  nlohmann::json tools_arr =
      nlohmann::json::array();

  if (agent_) {
    auto all_tools =
        agent_->GetToolDeclarations();
    for (const auto& td : all_tools) {
      // Filter by allowlist if set
      if (!allowed.empty()) {
        bool in_list = false;
        for (const auto& a : allowed) {
          if (a == td.name) {
            in_list = true;
            break;
          }
        }
        if (!in_list) continue;
      }

      nlohmann::json tool_j;
      tool_j["name"] = td.name;
      tool_j["description"] =
          td.description;
      tool_j["parameters"] =
          td.parameters;
      tools_arr.push_back(
          std::move(tool_j));
    }
  }

  nlohmann::json resp = {
      {"tools", tools_arr},
      {"count", tools_arr.size()}};
  std::string body = resp.dump();
  soup_message_set_status(msg, SOUP_STATUS_OK);
  soup_message_set_response(
      msg, "application/json",
      SOUP_MEMORY_COPY,
      body.c_str(),
      static_cast<gsize>(body.size()));
}

std::vector<std::string>
WebDashboard::LoadAppAllowedTools(
    const std::string& app_id) const {
  std::vector<std::string> allowed;
  if (app_id.empty()) return allowed;

  // Prevent path traversal
  if (app_id.find("..") != std::string::npos ||
      app_id.find('/') != std::string::npos)
    return allowed;

  std::string manifest_path =
      std::string(APP_DATA_DIR) +
      "/web/apps/" + app_id +
      "/manifest.json";

  std::ifstream mf(manifest_path);
  if (!mf.is_open()) return allowed;

  try {
    nlohmann::json manifest;
    mf >> manifest;
    if (manifest.contains("allowed_tools") &&
        manifest["allowed_tools"].is_array()) {
      for (const auto& t :
           manifest["allowed_tools"]) {
        if (t.is_string())
          allowed.push_back(
              t.get<std::string>());
      }
    }
  } catch (...) {}

  return allowed;
}

void WebDashboard::ApiBridgeData(
    SoupMessage* msg, GHashTable* query) {
  if (msg->method == SOUP_METHOD_GET) {
    // GET: read data by key
    std::string app_id;
    std::string key;
    if (query) {
      const char* aid =
          static_cast<const char*>(
              g_hash_table_lookup(
                  query, "app_id"));
      const char* k =
          static_cast<const char*>(
              g_hash_table_lookup(
                  query, "key"));
      if (aid) app_id = aid;
      if (k) key = k;
    }

    if (app_id.empty() || key.empty()) {
      std::string err =
          "{\"error\":\"app_id and key "
          "are required\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    // Prevent path traversal
    if (app_id.find("..") !=
            std::string::npos ||
        key.find("..") !=
            std::string::npos ||
        key.find('/') !=
            std::string::npos) {
      std::string err =
          "{\"error\":\"Invalid parameters\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    std::string data_path =
        std::string(APP_DATA_DIR) +
        "/web/apps/" + app_id +
        "/data/" + key + ".json";
    std::ifstream df(data_path);
    if (!df.is_open()) {
      std::string resp =
          "{\"key\":\"" + key +
          "\",\"value\":null}";
      soup_message_set_status(
          msg, SOUP_STATUS_OK);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          resp.c_str(),
          static_cast<gsize>(resp.size()));
      return;
    }

    std::string content(
        (std::istreambuf_iterator<char>(df)),
        std::istreambuf_iterator<char>());

    nlohmann::json resp;
    resp["key"] = key;
    try {
      resp["value"] =
          nlohmann::json::parse(content);
    } catch (...) {
      resp["value"] = content;
    }
    std::string body = resp.dump();
    soup_message_set_status(
        msg, SOUP_STATUS_OK);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        body.c_str(),
        static_cast<gsize>(body.size()));

  } else if (msg->method ==
             SOUP_METHOD_POST) {
    // POST: write data
    SoupMessageBody* req_body =
        msg->request_body;
    if (!req_body || !req_body->data ||
        req_body->length == 0) {
      std::string err =
          "{\"error\":\"Empty body\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    std::string payload(
        req_body->data, req_body->length);
    std::string app_id;
    std::string key;
    nlohmann::json val;

    try {
      auto j =
          nlohmann::json::parse(payload);
      app_id = j.value("app_id", "");
      key = j.value("key", "");
      if (j.contains("value"))
        val = j["value"];
    } catch (const std::exception& ex) {
      std::string err =
          "{\"error\":\"Invalid JSON\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    if (app_id.empty() || key.empty()) {
      std::string err =
          "{\"error\":\"app_id and key "
          "are required\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    // Prevent path traversal
    if (app_id.find("..") !=
            std::string::npos ||
        key.find("..") !=
            std::string::npos ||
        key.find('/') !=
            std::string::npos) {
      std::string err =
          "{\"error\":\"Invalid parameters\"}";
      soup_message_set_status(
          msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    std::string data_dir =
        std::string(APP_DATA_DIR) +
        "/web/apps/" + app_id + "/data";
    std::error_code ec;
    fs::create_directories(data_dir, ec);

    std::string data_path =
        data_dir + "/" + key + ".json";
    std::ofstream of(data_path);
    if (!of.is_open()) {
      std::string err =
          "{\"error\":\"Failed to write data\"}";
      soup_message_set_status(
          msg,
          SOUP_STATUS_INTERNAL_SERVER_ERROR);
      soup_message_set_response(
          msg, "application/json",
          SOUP_MEMORY_COPY,
          err.c_str(),
          static_cast<gsize>(err.size()));
      return;
    }

    of << val.dump();
    of.close();

    nlohmann::json resp = {
        {"status", "ok"},
    {"key", key}};
    std::string body = resp.dump();
    soup_message_set_status(
        msg, SOUP_STATUS_OK);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        body.c_str(),
        static_cast<gsize>(body.size()));
  } else {
    soup_message_set_status(
        msg,
        SOUP_STATUS_METHOD_NOT_ALLOWED);
  }
}

bool WebDashboard::CheckBridgeRateLimit(
    const std::string& app_id) {
  auto now = std::chrono::steady_clock::now();
  auto now_ms =
      std::chrono::duration_cast<
          std::chrono::milliseconds>(
          now.time_since_epoch())
          .count();

  std::lock_guard<std::mutex> lock(
      bridge_rate_mutex_);
  auto& timestamps = bridge_rate_[app_id];

  // Remove timestamps older than 1 second
  int64_t cutoff = now_ms - 1000;
  auto it = timestamps.begin();
  while (it != timestamps.end() &&
         *it < cutoff) {
    it = timestamps.erase(it);
  }

  // Check if over limit
  if (static_cast<int>(timestamps.size()) >=
      kBridgeRateLimit) {
    return false;
  }

  timestamps.push_back(now_ms);
  return true;
}

namespace {

struct BridgeChatCtx {
  WebDashboard* self;
  SoupMessage* msg;
  std::string app_id;
  std::string prompt;
  std::string result;
};

}  // namespace

void WebDashboard::ApiBridgeChat(
    SoupMessage* msg) {
  if (msg->method != SOUP_METHOD_POST) {
    soup_message_set_status(
        msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
    return;
  }

  SoupMessageBody* req_body = msg->request_body;
  if (!req_body || !req_body->data ||
      req_body->length == 0) {
    std::string err =
        "{\"error\":\"Empty body\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  std::string payload(
      req_body->data, req_body->length);
  std::string app_id;
  std::string prompt;

  try {
    auto j = nlohmann::json::parse(payload);
    app_id = j.value("app_id", "");
    prompt = j.value("prompt", "");
  } catch (...) {
    std::string err =
        "{\"error\":\"Invalid JSON\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  if (prompt.empty() || !agent_) {
    std::string err =
        "{\"error\":\"prompt required\"}";
    soup_message_set_status(
        msg, SOUP_STATUS_BAD_REQUEST);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  if (!running_.load()) {
    std::string err =
        "{\"error\":\"Shutting down\"}";
    soup_message_set_status(
        msg,
        SOUP_STATUS_SERVICE_UNAVAILABLE);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        err.c_str(),
        static_cast<gsize>(err.size()));
    return;
  }

  std::string session_id =
      "webapp_" + (app_id.empty()
                       ? "anonymous"
                       : app_id);

  // Pause msg and process asynchronously
  g_object_ref(msg);
  soup_server_pause_message(server_, msg);

  auto* ctx = new BridgeChatCtx{
      this, msg, app_id,
      prompt, ""};

  pending_workers_.fetch_add(1);
  std::thread([ctx, session_id]() {
    ctx->result =
        ctx->self->agent_->ProcessPrompt(
            session_id, ctx->prompt);
    g_main_context_invoke(
        ctx->self->context_,
        [](gpointer data) -> gboolean {
          auto* c =
              static_cast<BridgeChatCtx*>(
                  data);
          nlohmann::json resp = {
              {"status", "ok"},
              {"response", c->result}};
          std::string resp_str = resp.dump();
          soup_message_set_status(
              c->msg, SOUP_STATUS_OK);
          soup_message_set_response(
              c->msg, "application/json",
              SOUP_MEMORY_COPY,
              resp_str.c_str(),
              static_cast<gsize>(
                  resp_str.size()));
          soup_server_unpause_message(
              c->self->server_, c->msg);
          g_object_unref(c->msg);
          auto* owner = c->self;
          delete c;
          if (owner->pending_workers_
                  .fetch_sub(1) == 1) {
            std::lock_guard<std::mutex> lk(
                owner->workers_mutex_);
            owner->workers_cv_.notify_all();
          }
          return G_SOURCE_REMOVE;
        },
        ctx);
  }).detach();
}

}  // namespace tizenclaw
