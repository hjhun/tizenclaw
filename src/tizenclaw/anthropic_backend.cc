#include "anthropic_backend.hh"
#include "http_client.hh"

#include "../common/logging.hh"

namespace tizenclaw {


bool AnthropicBackend::Initialize(
    const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model",
      "claude-sonnet-4-20250514");
  if (api_key_.empty()) {
    LOG(ERROR) << "Anthropic API key is empty";
    return false;
  }
  LOG(INFO) << "Anthropic backend initialized (model: " << model_ << ")";
  return true;
}

nlohmann::json AnthropicBackend::ToAnthropicMessages(
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
        nlohmann::json content =
            nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          content.push_back({
              {"type", "tool_use"},
              {"id", tc.id},
              {"name", tc.name},
              {"input", tc.args}
          });
        }
        entry["content"] = content;
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {
          {"role", "user"},
          {"content", {{
              {"type", "tool_result"},
              {"tool_use_id", msg.tool_call_id},
              {"content",
               msg.tool_result.dump()}
          }}}
      };
    }
    msgs.push_back(entry);
  }
  return msgs;
}

nlohmann::json AnthropicBackend::ToAnthropicTools(
    const std::vector<LlmToolDecl>& tools) const {
  if (tools.empty()) return nullptr;
  nlohmann::json result = nlohmann::json::array();
  for (auto& t : tools) {
    result.push_back({
        {"name", t.name},
        {"description", t.description},
        {"input_schema", t.parameters}
    });
  }
  return result;
}

LlmResponse
AnthropicBackend::ParseAnthropicResponse(
    const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message =
          j["error"].value("message",
                           "Unknown error");
      return resp;
    }

    if (!j.contains("content") ||
        j["content"].empty()) {
      resp.success = false;
      resp.error_message = "Empty content";
      return resp;
    }

    resp.success = true;
    for (auto& block : j["content"]) {
      std::string type =
          block.value("type", "");
      if (type == "text") {
        if (!resp.text.empty())
          resp.text += "\n";
        resp.text +=
            block["text"].get<std::string>();
      } else if (type == "tool_use") {
        LlmToolCall tc;
        tc.id = block.value("id", "");
        tc.name = block["name"];
        tc.args = block["input"];
        resp.tool_calls.push_back(tc);
      }
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message =
        std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse AnthropicBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk) {
  nlohmann::json payload = {
      {"model", model_},
      {"max_tokens", 4096},
      {"messages",
       ToAnthropicMessages(messages)}
  };
  auto ant_tools = ToAnthropicTools(tools);
  if (!ant_tools.is_null()) {
    payload["tools"] = ant_tools;
  }

  std::string url =
      "https://api.anthropic.com/v1/messages";

  auto http_resp = HttpClient::Post(
      url,
      {{"Content-Type", "application/json"},
       {"x-api-key", api_key_},
       {"anthropic-version", "2023-06-01"}},
      payload.dump(),
      3, 10, 30, on_chunk); // Pass on_chunk callback

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    if (!http_resp.body.empty()) {
      try {
        auto ej =
            nlohmann::json::parse(http_resp.body);
        if (ej.contains("error")) {
          r.error_message += ": " +
              ej["error"].value("message", "");
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

  return ParseAnthropicResponse(http_resp.body);
}

} // namespace tizenclaw
