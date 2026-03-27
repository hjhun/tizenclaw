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
#include "session_store.hh"

#include <algorithm>
#include <chrono>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <regex>
#include <set>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace fs = std::filesystem;

SessionStore::SessionStore()
    : sessions_dir_("/opt/usr/share/tizenclaw/sessions") {}

void SessionStore::SetDirectory(const std::string& dir) { sessions_dir_ = dir; }

std::string SessionStore::GetDatePrefix() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%d");
  return oss.str();
}

std::string SessionStore::FindSessionFile(const std::string& dir,
                                          const std::string& session_id) const {
  // Look for *-{session_id}.md in dir
  std::string suffix = "-" + session_id + ".md";
  std::error_code ec;
  for (const auto& entry : fs::directory_iterator(dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    std::string name = entry.path().filename().string();
    if (name.size() > suffix.size() &&
        name.compare(name.size() - suffix.size(), suffix.size(), suffix) == 0) {
      return entry.path().string();
    }
  }
  return "";
}

std::string SessionStore::GetSessionPath(const std::string& session_id) const {
  // Reuse existing file if found
  std::string existing = FindSessionFile(sessions_dir_, session_id);
  if (!existing.empty()) return existing;

  // New file: YYYY-MM-DD-{session_id}.md
  return sessions_dir_ + "/" + GetDatePrefix() + "-" + session_id + ".md";
}

std::string SessionStore::GetLegacySessionPath(
    const std::string& session_id) const {
  return sessions_dir_ + "/" + session_id + ".json";
}

std::string SessionStore::GetLogsDir() const {
  // Go up one level from sessions/ to base dir
  std::string base = sessions_dir_;
  auto pos = base.rfind('/');
  if (pos != std::string::npos) {
    base = base.substr(0, pos);
  }
  return base + "/logs";
}

std::string SessionStore::GetUsageDir() const {
  std::string base = sessions_dir_;
  auto pos = base.rfind('/');
  if (pos != std::string::npos) {
    base = base.substr(0, pos);
  }
  return base + "/usage";
}

std::string SessionStore::GetDailyUsageDir() const {
  return GetUsageDir() + "/daily";
}

std::string SessionStore::GetMonthlyUsageDir() const {
  return GetUsageDir() + "/monthly";
}

std::string SessionStore::GetTimestamp() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%dT%H:%M:%S%z");
  return oss.str();
}

void SessionStore::EnsureDir(const std::string& dir) {
  std::error_code ec;
  fs::create_directories(dir, ec);
}

bool SessionStore::AtomicWrite(const std::string& path,
                               const std::string& content) {
  std::string tmp_path = path + ".tmp";
  std::ofstream out(tmp_path);
  if (!out.is_open()) {
    LOG(ERROR) << "Failed to open: " << tmp_path;
    return false;
  }
  out << content;
  out.close();
  if (out.fail()) {
    LOG(ERROR) << "Write failed: " << tmp_path;
    std::error_code ec;
    fs::remove(tmp_path, ec);
    return false;
  }
  std::error_code ec;
  fs::rename(tmp_path, path, ec);
  if (ec) {
    LOG(ERROR) << "Rename failed: " << tmp_path << " -> " << path;
    fs::remove(tmp_path, ec);
    return false;
  }
  return true;
}

// ------------------------------------------------
// Markdown Serialization
// ------------------------------------------------

std::string SessionStore::MessagesToMarkdown(
    const std::vector<LlmMessage>& history) const {
  std::ostringstream md;

  // YAML frontmatter
  md << "---\n";
  md << "message_count: " << history.size() << "\n";
  md << "updated: " << GetTimestamp() << "\n";
  md << "---\n\n";

  for (size_t i = 0; i < history.size(); ++i) {
    auto& msg = history[i];

    if (msg.role == "tool") {
      // Tool result message
      md << "## tool";
      if (!msg.tool_call_id.empty()) {
        md << " [" << msg.tool_call_id << "]";
      }
      if (!msg.tool_name.empty()) {
        md << " " << msg.tool_name;
      }
      md << "\n\n";
      if (!msg.tool_result.is_null()) {
        md << "```json\n";
        md << msg.tool_result.dump(2) << "\n";
        md << "```\n";
      }
    } else {
      // User or assistant message
      md << "## " << msg.role << "\n\n";

      if (!msg.text.empty()) {
        md << msg.text << "\n";
      }

      // Tool calls (assistant only)
      for (auto& tc : msg.tool_calls) {
        md << "\n### tool_call: " << tc.name;
        if (!tc.id.empty()) {
          md << " [" << tc.id << "]";
        }
        md << "\n\n";
        if (!tc.args.is_null() && !tc.args.empty()) {
          md << "```json\n";
          md << tc.args.dump(2) << "\n";
          md << "```\n";
        }
      }
    }

    // Separator between messages
    if (i + 1 < history.size()) {
      md << "\n---\n\n";
    }
  }

  return md.str();
}

std::vector<LlmMessage> SessionStore::MarkdownToMessages(
    const std::string& content) const {
  std::vector<LlmMessage> history;

  // Skip YAML frontmatter (between --- markers)
  std::string body = content;
  if (body.substr(0, 4) == "---\n") {
    auto end_pos = body.find("\n---\n", 4);
    if (end_pos != std::string::npos) {
      body = body.substr(end_pos + 5);
    }
  }

  // Split by --- separator lines
  std::vector<std::string> blocks;
  std::istringstream stream(body);
  std::string line;
  std::string current_block;

  while (std::getline(stream, line)) {
    if (line == "---") {
      if (!current_block.empty()) {
        blocks.push_back(current_block);
        current_block.clear();
      }
    } else {
      current_block += line + "\n";
    }
  }
  if (!current_block.empty()) {
    blocks.push_back(current_block);
  }

  for (auto& block : blocks) {
    // Trim leading/trailing whitespace
    size_t start = block.find_first_not_of(" \n\r");
    if (start == std::string::npos) continue;
    block = block.substr(start);

    // Must start with ## header
    if (block.substr(0, 3) != "## ") continue;

    // Parse header line
    auto header_end = block.find('\n');
    std::string header = block.substr(3, header_end - 3);
    std::string rest =
        (header_end != std::string::npos) ? block.substr(header_end + 1) : "";

    // Trim leading whitespace/newlines from
    // content after header
    {
      auto first = rest.find_first_not_of(" \n\r\t");
      if (first != std::string::npos) {
        rest = rest.substr(first);
      } else {
        rest.clear();
      }
    }

    LlmMessage msg;

    // Parse tool result:
    //   "tool [call_id] tool_name"
    if (header.substr(0, 4) == "tool") {
      msg.role = "tool";
      // Parse [call_id] and tool_name
      auto bracket_s = header.find('[');
      auto bracket_e = header.find(']');
      if (bracket_s != std::string::npos && bracket_e != std::string::npos) {
        msg.tool_call_id =
            header.substr(bracket_s + 1, bracket_e - bracket_s - 1);
        // tool_name after "] "
        if (bracket_e + 2 < header.size()) {
          msg.tool_name = header.substr(bracket_e + 2);
        }
      }
      // Extract JSON from fenced code block
      auto json_start = rest.find("```json\n");
      auto json_end = rest.find("\n```", json_start + 8);
      if (json_start != std::string::npos && json_end != std::string::npos) {
        std::string json_str =
            rest.substr(json_start + 8, json_end - json_start - 8);
        try {
          msg.tool_result = nlohmann::json::parse(json_str);
        } catch (...) {
          msg.tool_result = {{"output", json_str}};
        }
      }
    } else if (header == "user" || header == "assistant" ||
               header == "[compressed]") {
      // User, assistant, or compressed message
      if (header == "[compressed]") {
        msg.role = "assistant";
      } else {
        msg.role = header;
      }

      // Check for tool_call sub-sections
      // ### tool_call: name [id]
      auto tc_pos = rest.find("### tool_call:");
      if (tc_pos != std::string::npos) {
        // Text before first tool_call
        msg.text = rest.substr(0, tc_pos);
        // Trim trailing whitespace from text
        auto last = msg.text.find_last_not_of(" \n\r\t");
        if (last != std::string::npos) {
          msg.text = msg.text.substr(0, last + 1);
        } else {
          msg.text.clear();
        }

        // Parse each tool_call block
        std::string tc_rest = rest.substr(tc_pos);
        std::istringstream tc_stream(tc_rest);
        std::string tc_line;
        LlmToolCall current_tc;
        bool in_tc = false;
        std::string tc_json;
        bool in_json = false;

        while (std::getline(tc_stream, tc_line)) {
          if (tc_line.substr(0, 14) == "### tool_call:") {
            if (in_tc) {
              msg.tool_calls.push_back(current_tc);
              current_tc = LlmToolCall();
            }
            in_tc = true;
            // Parse "### tool_call: name [id]"
            std::string tc_header = tc_line.substr(15);
            auto bid = tc_header.find('[');
            auto eid = tc_header.find(']');
            if (bid != std::string::npos && eid != std::string::npos) {
              current_tc.name = tc_header.substr(0, bid - 1);
              current_tc.id = tc_header.substr(bid + 1, eid - bid - 1);
            } else {
              current_tc.name = tc_header;
            }
            // Trim name
            while (!current_tc.name.empty() && current_tc.name.back() == ' ') {
              current_tc.name.pop_back();
            }
          } else if (tc_line == "```json") {
            in_json = true;
            tc_json.clear();
          } else if (tc_line == "```" && in_json) {
            in_json = false;
            try {
              current_tc.args = nlohmann::json::parse(tc_json);
            } catch (...) {
              current_tc.args = nlohmann::json::object();
            }
          } else if (in_json) {
            tc_json += tc_line + "\n";
          }
        }
        if (in_tc) {
          msg.tool_calls.push_back(current_tc);
        }
      } else {
        // No tool calls — text only
        msg.text = rest;
        // Trim trailing whitespace
        auto last = msg.text.find_last_not_of(" \n\r\t");
        if (last != std::string::npos) {
          msg.text = msg.text.substr(0, last + 1);
        } else {
          msg.text.clear();
        }
      }

      // Mark compressed turns
      if (header == "[compressed]" && !msg.text.empty()) {
        msg.text = "[compressed] " + msg.text;
      }
    }

    if (msg.role.empty()) continue;
    history.push_back(msg);
  }

  return history;
}

// ------------------------------------------------
// Legacy JSON helpers (for migration)
// ------------------------------------------------

nlohmann::json SessionStore::MessageToJson(const LlmMessage& msg) {
  nlohmann::json j;
  j["role"] = msg.role;

  if (!msg.text.empty()) {
    j["text"] = msg.text;
  }

  if (!msg.tool_calls.empty()) {
    nlohmann::json tcs = nlohmann::json::array();
    for (auto& tc : msg.tool_calls) {
      tcs.push_back({{"id", tc.id}, {"name", tc.name}, {"args", tc.args}});
    }
    j["tool_calls"] = tcs;
  }

  if (!msg.tool_name.empty()) {
    j["tool_name"] = msg.tool_name;
  }

  if (!msg.tool_call_id.empty()) {
    j["tool_call_id"] = msg.tool_call_id;
  }

  if (!msg.tool_result.is_null()) {
    j["tool_result"] = msg.tool_result;
  }

  return j;
}

LlmMessage SessionStore::JsonToMessage(const nlohmann::json& j) {
  LlmMessage msg;
  msg.role = j.value("role", "");
  msg.text = j.value("text", "");
  msg.tool_name = j.value("tool_name", "");
  msg.tool_call_id = j.value("tool_call_id", "");

  if (j.contains("tool_result")) {
    msg.tool_result = j["tool_result"];
  }

  if (j.contains("tool_calls")) {
    for (auto& tc : j["tool_calls"]) {
      LlmToolCall call;
      call.id = tc.value("id", "");
      call.name = tc.value("name", "");
      if (tc.contains("args")) {
        call.args = tc["args"];
      }
      msg.tool_calls.push_back(call);
    }
  }

  return msg;
}

// ------------------------------------------------
// Session Save/Load (Markdown with JSON fallback)
// ------------------------------------------------

bool SessionStore::SaveSession(const std::string& session_id,
                               const std::vector<LlmMessage>& history) {
  if (session_id.empty() || history.empty()) {
    return false;
  }

  EnsureDir(sessions_dir_);

  std::string data = MessagesToMarkdown(history);

  // Check file size limit — trim oldest messages
  std::vector<LlmMessage> trimmed = history;
  while (data.size() > kMaxFileSize && trimmed.size() > 2) {
    trimmed.erase(trimmed.begin());
    data = MessagesToMarkdown(trimmed);
  }

  std::string path = GetSessionPath(session_id);
  if (!AtomicWrite(path, data)) {
    LOG(ERROR) << "Failed to save session: " << path;
    return false;
  }

  LOG(DEBUG) << "Session saved: " << session_id << " (" << trimmed.size()
             << " messages, " << data.size() << " bytes)";
  return true;
}

std::vector<LlmMessage> SessionStore::LoadSession(
    const std::string& session_id) {
  std::vector<LlmMessage> history;

  // Try loading Markdown first
  std::string md_path = GetSessionPath(session_id);
  std::ifstream md_in(md_path);
  if (md_in.is_open()) {
    std::ostringstream ss;
    ss << md_in.rdbuf();
    md_in.close();

    history = MarkdownToMessages(ss.str());
    if (!history.empty()) {
      LOG(INFO) << "Session loaded (md): " << session_id << " ("
                << history.size() << " messages)";
      return history;
    }
  }

  // Fallback: try legacy JSON and auto-migrate
  std::string json_path = GetLegacySessionPath(session_id);
  std::ifstream json_in(json_path);
  if (!json_in.is_open()) {
    return history;  // No saved session
  }

  try {
    nlohmann::json arr;
    json_in >> arr;
    json_in.close();

    if (!arr.is_array()) {
      LOG(WARNING) << "Invalid session file: " << json_path;
      return history;
    }

    for (auto& j : arr) {
      history.push_back(JsonToMessage(j));
    }

    LOG(INFO) << "Session loaded (json): " << session_id << " ("
              << history.size() << " messages)";

    // Auto-migrate: save as Markdown and remove
    // the old JSON file
    if (SaveSession(session_id, history)) {
      std::error_code ec;
      fs::remove(json_path, ec);
      LOG(INFO) << "Migrated session to md: " << session_id;
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse session " << json_path << ": " << e.what();
    history.clear();
  }

  return history;
}

void SessionStore::SanitizeHistory(std::vector<LlmMessage>& history) {
  // Build set of valid tool_call IDs from
  // assistant messages that have tool_calls
  std::set<std::string> valid_tool_call_ids;
  for (auto& msg : history) {
    if (msg.role == "assistant") {
      for (auto& tc : msg.tool_calls) {
        if (!tc.id.empty()) {
          valid_tool_call_ids.insert(tc.id);
        }
      }
    }
  }

  // Remove tool messages whose tool_call_id
  // is not in valid_tool_call_ids
  history.erase(
      std::remove_if(history.begin(), history.end(),
                     [&](const LlmMessage& msg) {
                       if (msg.role != "tool") return false;
                       if (msg.tool_call_id.empty())
                         return true;  // No ID = orphaned
                       return valid_tool_call_ids.find(msg.tool_call_id) ==
                              valid_tool_call_ids.end();
                     }),
      history.end());
}

void SessionStore::DeleteSession(const std::string& session_id) {
  // Delete both .md and legacy .json if exist
  std::error_code ec;
  std::string md_path = GetSessionPath(session_id);
  if (fs::remove(md_path, ec)) {
    LOG(INFO) << "Session deleted (md): " << session_id;
  }
  std::string json_path = GetLegacySessionPath(session_id);
  if (fs::remove(json_path, ec)) {
    LOG(INFO) << "Session deleted (json): " << session_id;
  }
}

// ------------------------------------------------
// Skill Execution Logging (Markdown table)
// ------------------------------------------------

void SessionStore::LogSkillExecution(const std::string& session_id,
                                     const std::string& skill_name,
                                     const nlohmann::json& args,
                                     const std::string& result,
                                     int duration_ms) {
  (void)args;
  (void)result;
  std::string logs_dir = GetLogsDir();
  EnsureDir(logs_dir);

  // Daily log file: YYYY-MM-DD.md
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream date_oss;
  date_oss << std::put_time(&tm_buf, "%Y-%m-%d");
  std::string date_str = date_oss.str();
  std::string log_path = logs_dir + "/" + date_str + ".md";

  // Check if file exists — add header if new
  bool is_new = !fs::exists(log_path);

  std::ofstream out(log_path, std::ios::app);
  if (!out.is_open()) {
    LOG(ERROR) << "Failed to open skill log: " << log_path;
    return;
  }

  if (is_new) {
    out << "# Skill Execution Log — " << date_str << "\n\n";
    out << "| Time | Session | Skill | " << "Duration |\n";
    out << "|------|---------|-------|-" << "--------|\n";
  }

  std::string ts = GetTimestamp();
  // Truncate session_id for table readability
  std::string short_sid = session_id;
  if (short_sid.size() > 16) {
    short_sid = short_sid.substr(0, 16) + "..";
  }

  out << "| " << ts << " | " << short_sid << " | " << skill_name << " | "
      << duration_ms << "ms |\n";
  out.close();

  LOG(DEBUG) << "Skill logged: " << skill_name << " (" << duration_ms << "ms)";
}

// ------------------------------------------------
// Token Usage Logging (Markdown per-session)
// ------------------------------------------------

void SessionStore::LogTokenUsage(const std::string& session_id,
                                 const std::string& model_name,
                                 int prompt_tokens, int completion_tokens) {
  if (session_id.empty()) return;

  std::string usage_dir = GetUsageDir();
  EnsureDir(usage_dir);

  // Find existing or create new with date prefix
  std::string usage_path = FindSessionFile(usage_dir, session_id);
  if (usage_path.empty()) {
    usage_path = usage_dir + "/" + GetDatePrefix() + "-" + session_id + ".md";
  }

  // Read existing usage summary if present
  TokenUsageSummary summary;
  std::ifstream in(usage_path);
  if (in.is_open()) {
    std::string line;
    // Parse frontmatter for totals
    bool in_frontmatter = false;
    while (std::getline(in, line)) {
      if (line == "---") {
        if (!in_frontmatter) {
          in_frontmatter = true;
          continue;
        } else {
          break;  // end of frontmatter
        }
      }
      if (in_frontmatter) {
        if (line.find("total_prompt_tokens:") == 0) {
          summary.total_prompt_tokens = std::stoi(line.substr(21));
        } else if (line.find("total_completion_tokens:") == 0) {
          summary.total_completion_tokens = std::stoi(line.substr(25));
        }
      }
    }
    in.close();
  }

  // Accumulate new usage
  summary.total_prompt_tokens += prompt_tokens;
  summary.total_completion_tokens += completion_tokens;

  // Append new entry to the table
  // Read existing table rows
  std::string existing_table;
  {
    std::ifstream re_in(usage_path);
    if (re_in.is_open()) {
      std::string content((std::istreambuf_iterator<char>(re_in)),
                          std::istreambuf_iterator<char>());
      re_in.close();

      // Extract table rows (lines starting with |
      // after the header)
      auto table_start = content.find("|---");
      if (table_start != std::string::npos) {
        auto after_sep = content.find('\n', table_start);
        if (after_sep != std::string::npos) {
          existing_table = content.substr(after_sep + 1);
        }
      }
    }
  }

  // Rebuild file
  std::ostringstream md;
  md << "---\n";
  md << "session_id: " << session_id << "\n";
  md << "total_prompt_tokens: " << summary.total_prompt_tokens << "\n";
  md << "total_completion_tokens: " << summary.total_completion_tokens << "\n";
  md << "updated: " << GetTimestamp() << "\n";
  md << "---\n\n";
  md << "# Token Usage — " << session_id << "\n\n";
  md << "| Time | Model | Prompt | " << "Completion |\n";
  md << "|------|-------|--------|-" << "-----------|\n";

  // Existing rows
  if (!existing_table.empty()) {
    md << existing_table;
  }

  // New row
  md << "| " << GetTimestamp() << " | " << model_name << " | " << prompt_tokens
     << " | " << completion_tokens << " |\n";

  AtomicWrite(usage_path, md.str());

  LOG(DEBUG) << "Token usage logged: " << model_name
             << " (prompt=" << prompt_tokens
             << ", completion=" << completion_tokens << ")";

  // --- Daily aggregate ---
  std::string daily_dir = GetDailyUsageDir();
  EnsureDir(daily_dir);
  auto now_t = std::chrono::system_clock::now();
  auto tt = std::chrono::system_clock::to_time_t(now_t);
  std::tm tm_daily{};
  localtime_r(&tt, &tm_daily);
  std::ostringstream date_oss;
  date_oss << std::put_time(&tm_daily, "%Y-%m-%d");
  std::string date_str = date_oss.str();
  std::string daily_path = daily_dir + "/" + date_str + ".md";

  // Read existing daily aggregate
  DailyUsageSummary daily;
  daily.date = date_str;
  std::string daily_rows;
  {
    std::ifstream din(daily_path);
    if (din.is_open()) {
      std::string dl;
      bool in_fm = false;
      bool past_hdr = false;
      while (std::getline(din, dl)) {
        if (dl == "---") {
          in_fm = !in_fm;
          continue;
        }
        if (in_fm) {
          if (dl.find("total_prompt_tokens:") == 0)
            daily.total_prompt_tokens = std::stoi(dl.substr(21));
          else if (dl.find("total_completion_tokens:") == 0)
            daily.total_completion_tokens = std::stoi(dl.substr(25));
          else if (dl.find("total_requests:") == 0)
            daily.total_requests = std::stoi(dl.substr(16));
        }
        if (dl.find("|---") == 0) {
          past_hdr = true;
          continue;
        }
        if (past_hdr && !dl.empty() && dl[0] == '|') {
          daily_rows += dl + "\n";
        }
      }
      din.close();
    }
  }

  daily.total_prompt_tokens += prompt_tokens;
  daily.total_completion_tokens += completion_tokens;
  daily.total_requests += 1;

  // Rebuild daily file
  std::ostringstream dmd;
  dmd << "---\n";
  dmd << "date: " << date_str << "\n";
  dmd << "total_prompt_tokens: " << daily.total_prompt_tokens << "\n";
  dmd << "total_completion_tokens: " << daily.total_completion_tokens << "\n";
  dmd << "total_requests: " << daily.total_requests << "\n";
  dmd << "updated: " << GetTimestamp() << "\n";
  dmd << "---\n\n";
  dmd << "# Daily Usage " << date_str << "\n\n";
  dmd << "| Time | Session | Backend | " << "Prompt | Completion |\n";
  dmd << "|------|---------|---------|--" << "------|------------|\n";
  if (!daily_rows.empty()) dmd << daily_rows;
  dmd << "| " << GetTimestamp() << " | " << session_id << " | " << model_name
      << " | " << prompt_tokens << " | " << completion_tokens << " |\n";

  AtomicWrite(daily_path, dmd.str());

  // --- Monthly aggregate ---
  std::string monthly_dir = GetMonthlyUsageDir();
  EnsureDir(monthly_dir);
  std::ostringstream month_oss;
  month_oss << std::put_time(&tm_daily, "%Y-%m");
  std::string month_str = month_oss.str();
  std::string monthly_path = monthly_dir + "/" + month_str + ".md";

  DailyUsageSummary monthly;
  monthly.date = month_str;
  {
    std::ifstream min(monthly_path);
    if (min.is_open()) {
      std::string ml;
      bool in_fm = false;
      while (std::getline(min, ml)) {
        if (ml == "---") {
          in_fm = !in_fm;
          continue;
        }
        if (in_fm) {
          if (ml.find("total_prompt_tokens:") == 0)
            monthly.total_prompt_tokens = std::stoi(ml.substr(21));
          else if (ml.find("total_completion_tokens:") == 0)
            monthly.total_completion_tokens = std::stoi(ml.substr(25));
          else if (ml.find("total_requests:") == 0)
            monthly.total_requests = std::stoi(ml.substr(16));
        }
      }
      min.close();
    }
  }

  monthly.total_prompt_tokens += prompt_tokens;
  monthly.total_completion_tokens += completion_tokens;
  monthly.total_requests += 1;

  std::ostringstream mmd;
  mmd << "---\n";
  mmd << "month: " << month_str << "\n";
  mmd << "total_prompt_tokens: " << monthly.total_prompt_tokens << "\n";
  mmd << "total_completion_tokens: " << monthly.total_completion_tokens << "\n";
  mmd << "total_requests: " << monthly.total_requests << "\n";
  mmd << "updated: " << GetTimestamp() << "\n";
  mmd << "---\n\n";
  mmd << "# Monthly Usage " << month_str << "\n\n";
  mmd << "**Prompt Tokens**: " << monthly.total_prompt_tokens << "  \n";
  mmd << "**Completion Tokens**: " << monthly.total_completion_tokens << "  \n";
  mmd << "**Total Requests**: " << monthly.total_requests << "\n";

  AtomicWrite(monthly_path, mmd.str());
}

TokenUsageSummary SessionStore::LoadTokenUsage(const std::string& session_id) const {
  TokenUsageSummary summary;

  std::string usage_path = FindSessionFile(GetUsageDir(), session_id);
  if (usage_path.empty()) return summary;
  std::ifstream in(usage_path);
  if (!in.is_open()) return summary;

  std::string line;
  bool in_frontmatter = false;
  bool past_header = false;

  while (std::getline(in, line)) {
    if (line == "---") {
      if (!in_frontmatter) {
        in_frontmatter = true;
        continue;
      } else {
        in_frontmatter = false;
        continue;
      }
    }
    if (in_frontmatter) {
      if (line.find("total_prompt_tokens:") == 0) {
        summary.total_prompt_tokens = std::stoi(line.substr(21));
      } else if (line.find("total_completion_tokens:") == 0) {
        summary.total_completion_tokens = std::stoi(line.substr(25));
      }
    }

    // Parse table rows
    if (line.find("|---") == 0) {
      past_header = true;
      continue;
    }
    if (past_header && !line.empty() && line[0] == '|') {
      // Parse: | time | model | prompt |
      //        completion |
      TokenUsageEntry entry;
      std::istringstream row(line);
      std::string cell;
      int col = 0;
      while (std::getline(row, cell, '|')) {
        // Trim
        size_t s = cell.find_first_not_of(" ");
        size_t e = cell.find_last_not_of(" ");
        if (s == std::string::npos) continue;
        cell = cell.substr(s, e - s + 1);

        switch (col) {
          case 1:
            entry.timestamp = cell;
            break;
          case 2:
            entry.model_name = cell;
            break;
          case 3:
            try {
              entry.prompt_tokens = std::stoi(cell);
            } catch (...) {
            }
            break;
          case 4:
            try {
              entry.completion_tokens = std::stoi(cell);
            } catch (...) {
            }
            break;
        }
        col++;
      }
      if (!entry.model_name.empty()) {
        summary.entries.push_back(entry);
      }
    }
  }

  in.close();
  return summary;
}

// ------------------------------------------------
// Daily/Monthly Aggregate Usage
// ------------------------------------------------

static DailyUsageSummary ParseAggregateMd(const std::string& path) {
  DailyUsageSummary summary;
  std::ifstream in(path);
  if (!in.is_open()) return summary;

  std::string line;
  bool in_fm = false;
  while (std::getline(in, line)) {
    if (line == "---") {
      in_fm = !in_fm;
      continue;
    }
    if (in_fm) {
      auto colon = line.find(':');
      if (colon == std::string::npos) continue;
      std::string key = line.substr(0, colon);
      std::string val = line.substr(colon + 2);
      if (key == "date" || key == "month")
        summary.date = val;
      else if (key == "total_prompt_tokens")
        summary.total_prompt_tokens = std::stoi(val);
      else if (key == "total_completion_tokens")
        summary.total_completion_tokens = std::stoi(val);
      else if (key == "total_requests")
        summary.total_requests = std::stoi(val);
    }
  }
  in.close();
  return summary;
}

DailyUsageSummary SessionStore::LoadDailyUsage(const std::string& date) const {
  std::string path = GetDailyUsageDir() + "/" + date + ".md";
  return ParseAggregateMd(path);
}

DailyUsageSummary SessionStore::LoadMonthlyUsage(
    const std::string& month) const {
  std::string path = GetMonthlyUsageDir() + "/" + month + ".md";
  return ParseAggregateMd(path);
}

}  // namespace tizenclaw
