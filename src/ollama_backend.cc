#include "ollama_backend.hh"
#include "http_client.hh"

#include <dlog.h>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Ollama"

bool OllamaBackend::Initialize(
    const nlohmann::json& config) {
  model_ = config.value("model", "llama3");
  endpoint_ = config.value("endpoint",
      "http://localhost:11434");
  dlog_print(DLOG_INFO, LOG_TAG,
             "Ollama backend initialized "
             "(model: %s, endpoint: %s)",
             model_.c_str(), endpoint_.c_str());
  return true;
}

nlohmann::json OllamaBackend::ToOllamaMessages(
    const std::vector<LlmMessage>& messages) const {
  nlohmann::json msgs = nlohmann::json::array();
  for (auto& msg : messages) {
    nlohmann::json entry;

    if (msg.role == "user") {
      entry = {{"role", "user"},
               {"content", msg.text}};
    } else if (msg.role == "assistant") {
      entry["role"] = "assistant";
      if (!msg.tool_calls.empty()) {
        nlohmann::json tcs =
            nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          tcs.push_back({
              {"function",
               {{"name", tc.name},
                {"arguments", tc.args}}}
          });
        }
        entry["tool_calls"] = tcs;
        entry["content"] = "";
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {{"role", "tool"},
               {"content",
                msg.tool_result.dump()}};
    }
    msgs.push_back(entry);
  }
  return msgs;
}

nlohmann::json OllamaBackend::ToOllamaTools(
    const std::vector<LlmToolDecl>& tools) const {
  if (tools.empty()) return nullptr;
  nlohmann::json result = nlohmann::json::array();
  for (auto& t : tools) {
    result.push_back({
        {"type", "function"},
        {"function",
         {{"name", t.name},
          {"description", t.description},
          {"parameters", t.parameters}}}
    });
  }
  return result;
}

LlmResponse OllamaBackend::ParseOllamaResponse(
    const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message =
          j["error"].get<std::string>();
      return resp;
    }

    // Ollama /api/chat response format
    if (!j.contains("message")) {
      resp.success = false;
      resp.error_message = "No message in response";
      return resp;
    }

    auto& msg = j["message"];
    resp.success = true;

    if (msg.contains("tool_calls") &&
        !msg["tool_calls"].empty()) {
      for (size_t i = 0; i < msg["tool_calls"].size(); ++i) {
        auto& tc = msg["tool_calls"][i];
        LlmToolCall call;
        call.id = "ollama_call_" +
                  std::to_string(i);
        call.name =
            tc["function"]["name"];
        call.args =
            tc["function"]["arguments"];
        resp.tool_calls.push_back(call);
      }
    }

    if (msg.contains("content")) {
      resp.text =
          msg["content"].get<std::string>();
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message =
        std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse OllamaBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools) {
  nlohmann::json payload = {
      {"model", model_},
      {"messages", ToOllamaMessages(messages)},
      {"stream", false}
  };
  auto ollama_tools = ToOllamaTools(tools);
  if (!ollama_tools.is_null()) {
    payload["tools"] = ollama_tools;
  }

  std::string url = endpoint_ + "/api/chat";

  auto http_resp = HttpClient::Post(
      url,
      {{"Content-Type", "application/json"}},
      payload.dump(),
      2,     // fewer retries for local
      5,     // faster connect for localhost
      120);  // longer timeout for local models

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    return r;
  }

  return ParseOllamaResponse(http_resp.body);
}
