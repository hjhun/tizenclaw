#include <dlog.h>
#include <fstream>
#include <sstream>
#include <sys/stat.h>

#include "session_store.hh"

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_SessionStore"

SessionStore::SessionStore()
    : sessions_dir_(
          "/opt/usr/share/tizenclaw/sessions") {
}

void SessionStore::SetDirectory(
    const std::string& dir) {
  sessions_dir_ = dir;
}

std::string SessionStore::GetSessionPath(
    const std::string& session_id) const {
  return sessions_dir_ + "/" + session_id + ".json";
}

nlohmann::json SessionStore::MessageToJson(
    const LlmMessage& msg) {
  nlohmann::json j;
  j["role"] = msg.role;

  if (!msg.text.empty()) {
    j["text"] = msg.text;
  }

  if (!msg.tool_calls.empty()) {
    nlohmann::json tcs = nlohmann::json::array();
    for (auto& tc : msg.tool_calls) {
      tcs.push_back({
          {"id", tc.id},
          {"name", tc.name},
          {"args", tc.args}
      });
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

LlmMessage SessionStore::JsonToMessage(
    const nlohmann::json& j) {
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

bool SessionStore::SaveSession(
    const std::string& session_id,
    const std::vector<LlmMessage>& history) {
  if (session_id.empty() || history.empty()) {
    return false;
  }

  // Ensure directory exists
  mkdir(sessions_dir_.c_str(), 0700);

  nlohmann::json arr = nlohmann::json::array();
  for (auto& msg : history) {
    arr.push_back(MessageToJson(msg));
  }

  std::string data = arr.dump(2);

  // Check file size limit — trim oldest messages
  while (data.size() > kMaxFileSize &&
         arr.size() > 2) {
    arr.erase(arr.begin());
    data = arr.dump(2);
  }

  std::string path = GetSessionPath(session_id);
  std::ofstream out(path);
  if (!out.is_open()) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Failed to save session: %s",
               path.c_str());
    return false;
  }

  out << data;
  out.close();

  dlog_print(DLOG_DEBUG, LOG_TAG,
             "Session saved: %s (%zu messages, "
             "%zu bytes)",
             session_id.c_str(), arr.size(),
             data.size());
  return true;
}

std::vector<LlmMessage> SessionStore::LoadSession(
    const std::string& session_id) {
  std::vector<LlmMessage> history;

  std::string path = GetSessionPath(session_id);
  std::ifstream in(path);
  if (!in.is_open()) {
    return history;  // No saved session
  }

  try {
    nlohmann::json arr;
    in >> arr;
    in.close();

    if (!arr.is_array()) {
      dlog_print(DLOG_WARN, LOG_TAG,
                 "Invalid session file: %s",
                 path.c_str());
      return history;
    }

    for (auto& j : arr) {
      history.push_back(JsonToMessage(j));
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "Session loaded: %s (%zu messages)",
               session_id.c_str(), history.size());
  } catch (const std::exception& e) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Failed to parse session %s: %s",
               path.c_str(), e.what());
    history.clear();
  }

  return history;
}

void SessionStore::DeleteSession(
    const std::string& session_id) {
  std::string path = GetSessionPath(session_id);
  if (remove(path.c_str()) == 0) {
    dlog_print(DLOG_INFO, LOG_TAG,
               "Session deleted: %s",
               session_id.c_str());
  }
}
