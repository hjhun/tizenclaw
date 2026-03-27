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
#include "ollama_backend.hh"

#include <chrono>
#include <iomanip>
#include <sstream>

#include "../../common/logging.hh"
#include "../infra/http_client.hh"

namespace tizenclaw {

bool OllamaBackend::Initialize(const nlohmann::json& config) {
  model_ = config.value("model", "llama3");
  endpoint_ = config.value("endpoint", "http://localhost:11434");
  LOG(INFO) << "Ollama backend initialized (model: " << model_
            << ", endpoint: " << endpoint_ << ")";
  return true;
}

nlohmann::json OllamaBackend::ToOllamaMessages(
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
          tcs.push_back(
              {{"function", {{"name", tc.name}, {"arguments", tc.args}}}});
        }
        entry["tool_calls"] = tcs;
        entry["content"] = "";
      } else {
        entry["content"] = msg.text;
      }
    } else if (msg.role == "tool") {
      entry = {{"role", "tool"}, {"content", msg.tool_result.dump()}};
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
    result.push_back({{"type", "function"},
                      {"function",
                       {{"name", t.name},
                        {"description", t.description},
                        {"parameters", t.parameters}}}});
  }
  return result;
}

LlmResponse OllamaBackend::ParseOllamaResponse(const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message = j["error"].get<std::string>();
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

    if (msg.contains("tool_calls") && !msg["tool_calls"].empty()) {
      for (size_t i = 0; i < msg["tool_calls"].size(); ++i) {
        auto& tc = msg["tool_calls"][i];
        LlmToolCall call;
        auto now = std::chrono::steady_clock::now().time_since_epoch().count();
        std::ostringstream oss;
        oss << "ollama_" << std::hex << now << "_" << i;
        call.id = oss.str();
        call.name = tc["function"]["name"];
        call.args = tc["function"]["arguments"];
        resp.tool_calls.push_back(call);
      }
    }

    if (msg.contains("content")) {
      resp.text = msg["content"].get<std::string>();
    }

    // Parse token usage
    resp.prompt_tokens = j.value("prompt_eval_count", 0);
    resp.completion_tokens = j.value("eval_count", 0);
    resp.total_tokens = resp.prompt_tokens + resp.completion_tokens;
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message = std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse OllamaBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  bool streaming = (on_chunk != nullptr);
  auto ollama_msgs = ToOllamaMessages(messages);
  if (!system_prompt.empty()) {
    ollama_msgs.insert(ollama_msgs.begin(),
                       {{"role", "system"}, {"content", system_prompt}});
  }
  nlohmann::json payload = {
      {"model", model_}, {"messages", ollama_msgs}, {"stream", streaming}};
  auto ollama_tools = ToOllamaTools(tools);
  if (!ollama_tools.is_null()) {
    payload["tools"] = ollama_tools;
    // Ollama doesn't support streaming with tools
    payload["stream"] = false;
    streaming = false;
  }

  std::string url = endpoint_ + "/api/chat";

  // NDJSON streaming state
  std::string ndjson_buffer;
  std::string accumulated_text;
  std::vector<LlmToolCall> accumulated_tools;

  std::function<void(const std::string&)> stream_cb = nullptr;
  if (streaming) {
    stream_cb = [&](const std::string& chunk) {
      ndjson_buffer += chunk;
      size_t pos;
      while ((pos = ndjson_buffer.find('\n')) != std::string::npos) {
        std::string line = ndjson_buffer.substr(0, pos);
        ndjson_buffer.erase(0, pos + 1);
        if (line.empty()) continue;

        try {
          auto j = nlohmann::json::parse(line);
          if (j.contains("message") && j["message"].contains("content")) {
            std::string text = j["message"]["content"].get<std::string>();
            if (!text.empty()) {
              accumulated_text += text;
              on_chunk(text);
            }
          }
        } catch (...) {
          // Skip malformed NDJSON lines
        }
      }
    };
  }

  auto http_resp = HttpClient::Post(url, {{"Content-Type", "application/json"}},
                                    payload.dump(),
                                    2,    // fewer retries for local
                                    5,    // faster connect for localhost
                                    120,  // longer timeout for local models
                                    stream_cb);

  if (!http_resp.success) {
    LlmResponse r;
    r.success = false;
    r.error_message = http_resp.error;
    if (!http_resp.body.empty()) {
      try {
        auto ej = nlohmann::json::parse(http_resp.body);
        if (ej.contains("error")) {
          std::string emsg = ej["error"].is_string()
                                 ? ej["error"].get<std::string>()
                                 : ej["error"].value("message", "");
          r.error_message += ": " + emsg;
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
    return resp;
  }

  return ParseOllamaResponse(http_resp.body);
}

}  // namespace tizenclaw
