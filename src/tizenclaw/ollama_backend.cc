#include "ollama_backend.hh"
#include "http_client.hh"

#include "../common/logging.hh"

namespace tizenclaw {


bool OllamaBackend::Initialize(
    const nlohmann::json& config) {
  model_ = config.value("model", "llama3");
  endpoint_ = config.value("endpoint",
      "http://localhost:11434");
  LOG(INFO) << "Ollama backend initialized (model: "
            << model_ << ", endpoint: " << endpoint_ << ")";
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
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk) {
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
      120,   // longer timeout for local models
      on_chunk); // Pass on_chunk callback

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    if (!http_resp.body.empty()) {
      try {
        auto ej =
            nlohmann::json::parse(http_resp.body);
        if (ej.contains("error")) {
          std::string emsg =
              ej["error"].is_string()
                  ? ej["error"].get<std::string>()
                  : ej["error"].value(
                        "message", "");
          r.error_message += ": " + emsg;
        }
      } catch (...) {
        r.error_message += ": " +
            http_resp.body.substr(
                0, std::min((size_t)200,
                            http_resp.body.size()));
      }
    }
    LOG(ERROR) << "API error: "
               << r.error_message;
    return r;
  }

  return ParseOllamaResponse(http_resp.body);
}

} // namespace tizenclaw
