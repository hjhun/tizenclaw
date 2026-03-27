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
#include "gemini_backend.hh"

#include <chrono>
#include <iomanip>
#include <sstream>

#include "../../common/logging.hh"
#include "../infra/http_client.hh"

namespace tizenclaw {

bool GeminiBackend::Initialize(const nlohmann::json& config) {
  api_key_ = config.value("api_key", "");
  model_ = config.value("model", "gemini-2.5-flash");
  if (api_key_.empty()) {
    LOG(ERROR) << "Gemini API key is empty";
    return false;
  }
  LOG(INFO) << "Gemini backend initialized (model: " << model_ << ")";
  return true;
}

nlohmann::json GeminiBackend::ToGeminiContents(
    const std::vector<LlmMessage>& messages) const {
  nlohmann::json contents = nlohmann::json::array();
  for (auto& msg : messages) {
    if (msg.role != "user" && msg.role != "assistant" && msg.role != "tool") {
      continue;
    }

    nlohmann::json entry;

    if (msg.role == "user") {
      entry["role"] = "user";
      entry["parts"] = {{{"text", msg.text}}};
    } else if (msg.role == "assistant") {
      entry["role"] = "model";
      if (!msg.tool_calls.empty()) {
        nlohmann::json parts = nlohmann::json::array();
        for (auto& tc : msg.tool_calls) {
          parts.push_back(
              {{"functionCall", {{"name", tc.name}, {"args", tc.args}}}});
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
        fn_resp = {{"output", msg.tool_result.dump()}};
      }
      entry["parts"] = {{{"functionResponse",
                          {{"name", msg.tool_name}, {"response", fn_resp}}}}};
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
    decls.push_back({{"name", t.name},
                     {"description", t.description},
                     {"parameters", t.parameters}});
  }
  return {{{"functionDeclarations", decls}}};
}

LlmResponse GeminiBackend::ParseGeminiResponse(const std::string& body) const {
  LlmResponse resp;
  try {
    auto j = nlohmann::json::parse(body);

    if (j.contains("error")) {
      resp.success = false;
      resp.error_message = j["error"].value("message", "Unknown API error");
      return resp;
    }

    if (!j.contains("candidates") || j["candidates"].empty()) {
      resp.success = false;
      resp.error_message = "Empty candidates";
      return resp;
    }

    auto& parts = j["candidates"][0]["content"]["parts"];
    resp.success = true;

    for (size_t i = 0; i < parts.size(); ++i) {
      auto& part = parts[i];
      if (part.contains("functionCall")) {
        LlmToolCall tc;
        auto now = std::chrono::steady_clock::now().time_since_epoch().count();
        std::ostringstream oss;
        oss << "gemini_" << std::hex << now << "_" << i;
        tc.id = oss.str();
        tc.name = part["functionCall"]["name"];
        tc.args = part["functionCall"]["args"];
        resp.tool_calls.push_back(tc);
      } else if (part.contains("text")) {
        if (!resp.text.empty()) resp.text += "\n";
        resp.text += part["text"].get<std::string>();
      }
    }

    // Parse token usage from usageMetadata
    if (j.contains("usageMetadata")) {
      auto& um = j["usageMetadata"];
      resp.prompt_tokens = um.value("promptTokenCount", 0);
      resp.completion_tokens = um.value("candidatesTokenCount", 0);
      resp.total_tokens = um.value("totalTokenCount", 0);
    }
  } catch (const std::exception& e) {
    resp.success = false;
    resp.error_message = std::string("Parse error: ") + e.what();
  }
  return resp;
}

LlmResponse GeminiBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  nlohmann::json payload = {{"contents", ToGeminiContents(messages)}};
  if (!system_prompt.empty()) {
    payload["system_instruction"] = {{"parts", {{{"text", system_prompt}}}}};
  }
  auto gemini_tools = ToGeminiTools(tools);
  if (!gemini_tools.is_null()) {
    payload["tools"] = gemini_tools;
  }

  bool streaming = (on_chunk != nullptr);
  std::string url =
      "https://generativelanguage.googleapis.com"
      "/v1beta/models/" +
      model_;
  if (streaming) {
    url += ":streamGenerateContent?alt=sse&key=" + api_key_;
  } else {
    url += ":generateContent?key=" + api_key_;
  }

  // SSE streaming state
  std::string sse_buffer;
  std::string accumulated_text;
  std::vector<LlmToolCall> accumulated_tools;
  size_t tool_idx = 0;

  std::function<void(const std::string&)> stream_cb = nullptr;
  if (streaming) {
    stream_cb = [&](const std::string& chunk) {
      sse_buffer += chunk;
      size_t pos;
      while ((pos = sse_buffer.find('\n')) != std::string::npos) {
        std::string line = sse_buffer.substr(0, pos);
        sse_buffer.erase(0, pos + 1);
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.empty()) continue;

        if (line.rfind("data: ", 0) != 0) continue;
        std::string data = line.substr(6);

        try {
          auto j = nlohmann::json::parse(data);
          if (!j.contains("candidates") || j["candidates"].empty()) continue;
          auto& parts = j["candidates"][0]["content"]["parts"];

          for (size_t i = 0; i < parts.size(); ++i) {
            auto& part = parts[i];
            if (part.contains("text")) {
              std::string text_delta = part["text"].get<std::string>();
              accumulated_text += text_delta;
              on_chunk(text_delta);
            } else if (part.contains("functionCall")) {
              LlmToolCall tc;
              auto now =
                  std::chrono::steady_clock ::now().time_since_epoch().count();
              std::ostringstream oss;
              oss << "gemini_" << std::hex << now << "_" << tool_idx++;
              tc.id = oss.str();
              tc.name = part["functionCall"]["name"];
              tc.args = part["functionCall"]["args"];
              accumulated_tools.push_back(tc);
            }
          }
        } catch (...) {
          // Skip malformed SSE events
        }
      }
    };
  }

  auto http_resp = HttpClient::Post(url, {{"Content-Type", "application/json"}},
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
    resp.tool_calls = accumulated_tools;
    return resp;
  }

  return ParseGeminiResponse(http_resp.body);
}

}  // namespace tizenclaw
