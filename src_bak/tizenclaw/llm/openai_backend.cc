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
#include "openai_backend.hh"

#include <sstream>

#include "../../common/logging.hh"
#include "../infra/http_client.hh"

namespace tizenclaw {

bool OpenAiBackend::Initialize(const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model", "gpt-4o");
  endpoint_ = config.value("endpoint", "https://api.openai.com/v1");
  name_ = config.value("provider_name", "openai");

  if (api_key_.empty()) {
    LOG(ERROR) << name_ << " API key is empty";
    return false;
  }
  LOG(INFO) << name_ << " backend initialized (model: " << model_
            << ", endpoint: " << endpoint_ << ")";
  return true;
}

nlohmann::json OpenAiBackend::ToOpenAiMessages(
    const std::vector<LlmMessage>& messages) const {
  nlohmann::json msgs = nlohmann::json::array();
  for (auto& msg : messages) {
    if (msg.role != "user" && msg.role != "assistant" && msg.role != "tool") {
      continue;
    }

    nlohmann::json entry;

    if (msg.role == "user") {
      entry = {{"role", "user"}, {"content", msg.text}};
    } else if (msg.role == "assistant") {
      entry["role"] = "assistant";
      if (!msg.tool_calls.empty()) {
        nlohmann::json tcs = nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          tcs.push_back({{"id", tc.id},
                         {"type", "function"},
                         {"function",
                          {{"name", tc.name}, {"arguments", tc.args.dump()}}}});
        }
        entry["tool_calls"] = tcs;
        // Content can be null when tool_calls
        entry["content"] = nullptr;
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {{"role", "tool"},
               {"tool_call_id", msg.tool_call_id},
               {"content", msg.tool_result.dump()}};
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
    result.push_back({{"type", "function"},
                      {"function",
                       {{"name", t.name},
                        {"description", t.description},
                        {"parameters", t.parameters}}}});
  }
  return result;
}

LlmResponse OpenAiBackend::ParseOpenAiResponse(const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message = j["error"].value("message", "Unknown error");
      return resp;
    }

    if (!j.contains("choices") || j["choices"].empty()) {
      resp.success = false;
      resp.error_message = "Empty choices";
      return resp;
    }

    auto& msg = j["choices"][0]["message"];
    resp.success = true;

    if (msg.contains("tool_calls") && !msg["tool_calls"].empty()) {
      for (auto& tc : msg["tool_calls"]) {
        LlmToolCall call;
        call.id = tc.value("id", "");
        call.name = tc["function"]["name"];
        try {
          call.args = nlohmann::json::parse(
              tc["function"]["arguments"].get<std::string>());
        } catch (...) {
          call.args = tc["function"]["arguments"];
        }
        resp.tool_calls.push_back(call);
      }
    }

    if (msg.contains("content") && !msg["content"].is_null()) {
      resp.text = msg["content"].get<std::string>();
    }

    // Parse token usage
    if (j.contains("usage")) {
      auto& u = j["usage"];
      resp.prompt_tokens = u.value("prompt_tokens", 0);
      resp.completion_tokens = u.value("completion_tokens", 0);
      resp.total_tokens = u.value("total_tokens", 0);
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message = std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse OpenAiBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  auto oai_msgs = ToOpenAiMessages(messages);
  if (!system_prompt.empty()) {
    nlohmann::json sys_msg;
    sys_msg["role"] = "system";
    sys_msg["content"] = system_prompt;
    oai_msgs.insert(oai_msgs.begin(), sys_msg);
  }
  nlohmann::json payload = {{"model", model_}, {"messages", oai_msgs}};
  auto oai_tools = ToOpenAiTools(tools);
  if (!oai_tools.is_null()) {
    payload["tools"] = oai_tools;
  }

  bool streaming = (on_chunk != nullptr);
  if (streaming) {
    payload["stream"] = true;
  }

  std::string url = endpoint_ + "/chat/completions";

  // For streaming: SSE line-buffer parser
  std::string sse_buffer;
  std::string accumulated_text;
  // Accumulate tool_call fragments (index -> {id, name, args_str})
  struct ToolCallAccum {
    std::string id;
    std::string name;
    std::string arguments;
  };
  std::map<int, ToolCallAccum> tc_accum;

  std::function<void(const std::string&)> stream_cb = nullptr;
  if (streaming) {
    stream_cb = [&](const std::string& chunk) {
      sse_buffer += chunk;
      // Process complete lines
      size_t pos;
      while ((pos = sse_buffer.find('\n')) != std::string::npos) {
        std::string line = sse_buffer.substr(0, pos);
        sse_buffer.erase(0, pos + 1);
        // Remove trailing \r
        if (!line.empty() && line.back() == '\r') {
          line.pop_back();
        }
        if (line.empty()) continue;

        // Parse SSE data lines
        if (line.rfind("data: ", 0) != 0) continue;
        std::string data = line.substr(6);
        if (data == "[DONE]") continue;

        try {
          auto j = nlohmann::json::parse(data);
          if (!j.contains("choices") || j["choices"].empty()) continue;
          auto& delta = j["choices"][0]["delta"];

          // Text content delta
          if (delta.contains("content") && !delta["content"].is_null()) {
            std::string text_delta = delta["content"].get<std::string>();
            accumulated_text += text_delta;
            on_chunk(text_delta);
          }

          // Tool call delta accumulation
          if (delta.contains("tool_calls")) {
            for (auto& tc_delta : delta["tool_calls"]) {
              int idx = tc_delta.value("index", 0);
              if (tc_delta.contains("id")) {
                tc_accum[idx].id = tc_delta["id"].get<std::string>();
              }
              if (tc_delta.contains("function")) {
                auto& fn = tc_delta["function"];
                if (fn.contains("name")) {
                  tc_accum[idx].name = fn["name"].get<std::string>();
                }
                if (fn.contains("arguments")) {
                  tc_accum[idx].arguments += fn["arguments"].get<std::string>();
                }
              }
            }
          }
        } catch (...) {
          // Skip malformed SSE events
        }
      }
    };
  }

  auto http_resp = HttpClient::Post(url,
                                    {{"Content-Type", "application/json"},
                                     {"Authorization", "Bearer " + api_key_}},
                                    payload.dump(), 3, 10, 120, stream_cb);

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    if (!http_resp.body.empty()) {
      try {
        auto ej = nlohmann::json::parse(http_resp.body);
        if (ej.contains("error")) {
          r.error_message += ": " + ej["error"].value("message", "");
        }
      } catch (...) {
        r.error_message +=
            ": " + http_resp.body.substr(
                       0, std::min((size_t)200, http_resp.body.size()));
      }
    }
    LOG(ERROR) << "API error: " << r.error_message;
    return r;
  }

  // Reconstruct response from streaming
  if (streaming) {
    LlmResponse resp;
    resp.success = true;
    resp.text = accumulated_text;
    for (auto& [idx, tc] : tc_accum) {
      LlmToolCall call;
      call.id = tc.id;
      call.name = tc.name;
      try {
        call.args = nlohmann::json::parse(tc.arguments);
      } catch (...) {
        call.args = tc.arguments;
      }
      resp.tool_calls.push_back(call);
    }
    return resp;
  }

  return ParseOpenAiResponse(http_resp.body);
}

}  // namespace tizenclaw
