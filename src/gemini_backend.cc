#include "gemini_backend.hh"
#include "http_client.hh"

#include <dlog.h>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Gemini"

bool GeminiBackend::Initialize(
    const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model",
                         "gemini-2.5-flash");
  if (api_key_.empty()) {
    dlog_print(DLOG_ERROR, LOG_TAG,
               "Gemini API key is empty");
    return false;
  }
  dlog_print(DLOG_INFO, LOG_TAG,
             "Gemini backend initialized "
             "(model: %s)", model_.c_str());
  return true;
}

nlohmann::json GeminiBackend::ToGeminiContents(
    const std::vector<LlmMessage>& messages) const {
  nlohmann::json contents = nlohmann::json::array();
  for (auto& msg : messages) {
    nlohmann::json entry;

    if (msg.role == "user") {
      entry["role"] = "user";
      entry["parts"] = {{{"text", msg.text}}};
    } else if (msg.role == "assistant") {
      entry["role"] = "model";
      if (!msg.tool_calls.empty()) {
        nlohmann::json parts =
            nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          parts.push_back({
              {"functionCall",
               {{"name", tc.name},
                {"args", tc.args}}}
          });
        }
        entry["parts"] = parts;
      } else {
        entry["parts"] = {{{"text", msg.text}}};
      }
    } else if (msg.role == "tool") {
      entry["role"] = "function";
      nlohmann::json fn_resp;
      try {
        fn_resp = msg.tool_result;
      } catch (...) {
        fn_resp = {{"output",
                    msg.tool_result.dump()}};
      }
      entry["parts"] = {{
          {"functionResponse",
           {{"name", msg.tool_name},
            {"response", fn_resp}}}
      }};
    }
    contents.push_back(entry);
  }
  return contents;
}

nlohmann::json GeminiBackend::ToGeminiTools(
    const std::vector<LlmToolDecl>& tools) const {
  if (tools.empty()) return nullptr;
  nlohmann::json decls = nlohmann::json::array();
  for (auto& t : tools) {
    decls.push_back({
        {"name", t.name},
        {"description", t.description},
        {"parameters", t.parameters}
    });
  }
  return {{{"functionDeclarations", decls}}};
}

LlmResponse GeminiBackend::ParseGeminiResponse(
    const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message =
          j["error"].value("message",
                           "Unknown API error");
      return resp;
    }

    if (!j.contains("candidates") ||
        j["candidates"].empty()) {
      resp.success = false;
      resp.error_message = "Empty candidates";
      return resp;
    }

    auto& parts =
        j["candidates"][0]["content"]["parts"];
    resp.success = true;

    for (size_t i = 0; i < parts.size(); ++i) {
      auto& part = parts[i];
      if (part.contains("functionCall")) {
        LlmToolCall tc;
        tc.id = "gemini_call_" +
                std::to_string(i);
        tc.name =
            part["functionCall"]["name"];
        tc.args =
            part["functionCall"]["args"];
        resp.tool_calls.push_back(tc);
      } else if (part.contains("text")) {
        if (!resp.text.empty())
          resp.text += "\n";
        resp.text +=
            part["text"].get<std::string>();
      }
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message =
        std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse GeminiBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools) {
  nlohmann::json payload = {
      {"contents", ToGeminiContents(messages)}
  };
  auto gemini_tools = ToGeminiTools(tools);
  if (!gemini_tools.is_null()) {
    payload["tools"] = gemini_tools;
  }

  std::string url =
      "https://generativelanguage.googleapis.com"
      "/v1beta/models/" + model_ +
      ":generateContent?key=" + api_key_;

  auto http_resp = HttpClient::Post(
      url,
      {{"Content-Type", "application/json"}},
      payload.dump());

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    return r;
  }

  return ParseGeminiResponse(http_resp.body);
}
