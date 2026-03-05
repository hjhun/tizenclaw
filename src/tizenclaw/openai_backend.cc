#include "openai_backend.hh"
#include "http_client.hh"

#include "../common/logging.hh"

namespace tizenclaw {


bool OpenAiBackend::Initialize(
    const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model", "gpt-4o");
  endpoint_ = config.value("endpoint",
      "https://api.openai.com/v1");
  name_ = config.value("provider_name", "openai");

  if (api_key_.empty()) {
    LOG(ERROR) << name_ << " API key is empty";
    return false;
  }
  LOG(INFO) << name_ << " backend initialized (model: "
            << model_ << ", endpoint: " << endpoint_ << ")";
  return true;
}

nlohmann::json OpenAiBackend::ToOpenAiMessages(
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
              {"id", tc.id},
              {"type", "function"},
              {"function",
               {{"name", tc.name},
                {"arguments", tc.args.dump()}}}
          });
        }
        entry["tool_calls"] = tcs;
        // Content can be null when tool_calls
        entry["content"] = nullptr;
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {
          {"role", "tool"},
          {"tool_call_id", msg.tool_call_id},
          {"content", msg.tool_result.dump()}
      };
    }
    msgs.push_back(entry);
  }
  return msgs;
}

nlohmann::json OpenAiBackend::ToOpenAiTools(
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

LlmResponse OpenAiBackend::ParseOpenAiResponse(
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

    if (!j.contains("choices") ||
        j["choices"].empty()) {
      resp.success = false;
      resp.error_message = "Empty choices";
      return resp;
    }

    auto& msg = j["choices"][0]["message"];
    resp.success = true;

    if (msg.contains("tool_calls") &&
        !msg["tool_calls"].empty()) {
      for (auto& tc : msg["tool_calls"]) {
        LlmToolCall call;
        call.id = tc.value("id", "");
        call.name =
            tc["function"]["name"];
        try {
          call.args = nlohmann::json::parse(
              tc["function"]["arguments"]
                  .get<std::string>());
        } catch (...) {
          call.args =
              tc["function"]["arguments"];
        }
        resp.tool_calls.push_back(call);
      }
    }

    if (msg.contains("content") &&
        !msg["content"].is_null()) {
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

LlmResponse OpenAiBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk) {
  nlohmann::json payload = {
      {"model", model_},
      {"messages", ToOpenAiMessages(messages)}
  };
  auto oai_tools = ToOpenAiTools(tools);
  if (!oai_tools.is_null()) {
    payload["tools"] = oai_tools;
  }

  std::string url =
      endpoint_ + "/chat/completions";

  auto http_resp = HttpClient::Post(
      url,
      {{"Content-Type", "application/json"},
       {"Authorization",
        "Bearer " + api_key_}},
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

  return ParseOpenAiResponse(http_resp.body);
}

} // namespace tizenclaw
