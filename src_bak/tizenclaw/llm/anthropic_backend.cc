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
#include "anthropic_backend.hh"

#include <sstream>

#include "../../common/logging.hh"
#include "../infra/http_client.hh"

namespace tizenclaw {

bool AnthropicBackend::Initialize(const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model", "claude-sonnet-4-20250514");
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
    if (msg.role != "user" && msg.role != "assistant" && msg.role != "tool") {
      continue;
    }

    nlohmann::json entry;

    if (msg.role == "user") {
      entry = {{"role", "user"}, {"content", msg.text}};
    } else if (msg.role == "assistant") {
      entry["role"] = "assistant";
      if (!msg.tool_calls.empty()) {
        nlohmann::json content = nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          content.push_back({{"type", "tool_use"},
                             {"id", tc.id},
                             {"name", tc.name},
                             {"input", tc.args}});
        }
        entry["content"] = content;
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {{"role", "user"},
               {"content",
                {{{"type", "tool_result"},
                  {"tool_use_id", msg.tool_call_id},
                  {"content", msg.tool_result.dump()}}}}};
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
    result.push_back({{"name", t.name},
                      {"description", t.description},
                      {"input_schema", t.parameters}});
  }
  return result;
}

LlmResponse AnthropicBackend::ParseAnthropicResponse(
    const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message = j["error"].value("message", "Unknown error");
      return resp;
    }

    if (!j.contains("content") || j["content"].empty()) {
      resp.success = false;
      resp.error_message = "Empty content";
      return resp;
    }

    resp.success = true;
    for (auto& block : j["content"]) {
      std::string type = block.value("type", "");
      if (type == "text") {
        if (!resp.text.empty()) resp.text += "\n";
        resp.text += block["text"].get<std::string>();
      } else if (type == "tool_use") {
        LlmToolCall tc;
        tc.id = block.value("id", "");
        tc.name = block["name"];
        tc.args = block["input"];
        resp.tool_calls.push_back(tc);
      }
    }

    // Parse token usage
    if (j.contains("usage")) {
      auto& u = j["usage"];
      resp.prompt_tokens = u.value("input_tokens", 0);
      resp.completion_tokens = u.value("output_tokens", 0);
      resp.total_tokens = resp.prompt_tokens + resp.completion_tokens;
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message = std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse AnthropicBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  nlohmann::json payload = {{"model", model_},
                            {"max_tokens", 4096},
                            {"messages", ToAnthropicMessages(messages)}};
  if (!system_prompt.empty()) {
    payload["system"] = system_prompt;
  }
  auto ant_tools = ToAnthropicTools(tools);
  if (!ant_tools.is_null()) {
    payload["tools"] = ant_tools;
  }

  bool streaming = (on_chunk != nullptr);
  if (streaming) {
    payload["stream"] = true;
  }

  std::string url = "https://api.anthropic.com/v1/messages";

  // SSE streaming state
  std::string sse_buffer;
  std::string current_event;
  std::string accumulated_text;
  struct ToolAccum {
    std::string id;
    std::string name;
    std::string input_json;
  };
  std::vector<ToolAccum> tool_accums;
  int current_tool_idx = -1;

  std::function<void(const std::string&)> stream_cb = nullptr;
  if (streaming) {
    stream_cb = [&](const std::string& chunk) {
      sse_buffer += chunk;
      size_t pos;
      while ((pos = sse_buffer.find('\n')) != std::string::npos) {
        std::string line = sse_buffer.substr(0, pos);
        sse_buffer.erase(0, pos + 1);
        if (!line.empty() && line.back() == '\r') line.pop_back();

        // Track event type
        if (line.rfind("event: ", 0) == 0) {
          current_event = line.substr(7);
          continue;
        }

        if (line.rfind("data: ", 0) != 0) {
          if (line.empty()) current_event.clear();
          continue;
        }
        std::string data = line.substr(6);

        try {
          auto j = nlohmann::json::parse(data);

          if (current_event == "content_block_start") {
            auto& cb = j["content_block"];
            std::string type = cb.value("type", "");
            if (type == "tool_use") {
              ToolAccum ta;
              ta.id = cb.value("id", "");
              ta.name = cb.value("name", "");
              tool_accums.push_back(ta);
              current_tool_idx = tool_accums.size() - 1;
            } else {
              current_tool_idx = -1;
            }
          } else if (current_event == "content_block_delta") {
            auto& delta = j["delta"];
            std::string type = delta.value("type", "");
            if (type == "text_delta") {
              std::string text = delta["text"].get<std::string>();
              accumulated_text += text;
              on_chunk(text);
            } else if (type == "input_json_delta" && current_tool_idx >= 0) {
              tool_accums[current_tool_idx].input_json +=
                  delta["partial_json"].get<std::string>();
            }
          }
        } catch (...) {
          // Skip malformed events
        }
      }
    };
  }

  auto http_resp = HttpClient::Post(url,
                                    {{"Content-Type", "application/json"},
                                     {"x-api-key", api_key_},
                                     {"anthropic-version", "2023-06-01"}},
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

  if (streaming) {
    LlmResponse resp;
    resp.success = true;
    resp.text = accumulated_text;
    for (auto& ta : tool_accums) {
      LlmToolCall tc;
      tc.id = ta.id;
      tc.name = ta.name;
      try {
        tc.args = nlohmann::json::parse(ta.input_json);
      } catch (...) {
        tc.args = ta.input_json;
      }
      resp.tool_calls.push_back(tc);
    }
    return resp;
  }

  return ParseAnthropicResponse(http_resp.body);
}

}  // namespace tizenclaw
