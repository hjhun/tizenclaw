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
#ifndef LLM_BACKEND_HH
#define LLM_BACKEND_HH

#include <functional>
#include <json.hpp>
#include <memory>
#include <string>
#include <vector>

namespace tizenclaw {

// --------------------------------------------------
// Unified message structures (provider-agnostic)
// --------------------------------------------------

struct LlmToolCall {
  std::string id;  // provider-assigned ID (e.g. "call_abc123")
  std::string name;
  nlohmann::json args;
};

struct LlmMessage {
  std::string role;  // "user", "assistant", "tool"
  std::string text;
  // When role == "assistant" and a tool call is made
  std::vector<LlmToolCall> tool_calls;
  // When role == "tool" (function result)
  std::string tool_name;
  std::string tool_call_id;  // ID of the tool_call this result responds to
  nlohmann::json tool_result;
};

struct LlmResponse {
  bool success = false;
  std::string text;
  std::string error_message;
  std::vector<LlmToolCall> tool_calls;

  // Token usage (parsed from API response)
  int prompt_tokens = 0;
  int completion_tokens = 0;
  int total_tokens = 0;

  // HTTP status for fallback decisions
  int http_status = 0;

  [[nodiscard]] bool HasToolCalls() const { return !tool_calls.empty(); }
};

// Tool declaration for function calling
struct LlmToolDecl {
  std::string name;
  std::string description;
  nlohmann::json parameters;  // JSON Schema
};

// --------------------------------------------------
// Abstract LLM Backend Interface
// --------------------------------------------------

class LlmBackend {
 public:
  virtual ~LlmBackend() = default;

  // Initialize with provider-specific config
  [[nodiscard]] virtual bool Initialize(const nlohmann::json& config) = 0;

  // Send a chat request. Returns unified response.
  [[nodiscard]] virtual LlmResponse Chat(
      const std::vector<LlmMessage>& messages,
      const std::vector<LlmToolDecl>& tools,
      std::function<void(const std::string&)> on_chunk = nullptr,
      const std::string& system_prompt = "") = 0;

  // Provider name (e.g. "gemini", "openai")
  [[nodiscard]] virtual std::string GetName() const = 0;

  // Cleanup
  virtual void Shutdown() {}
};

// --------------------------------------------------
// Factory: create a backend by name
// --------------------------------------------------

class LlmBackendFactory {
 public:
  // Supported names: gemini, openai, anthropic,
  // xai, ollama
  [[nodiscard]] static std::unique_ptr<LlmBackend> Create(
      const std::string& name);
};

}  // namespace tizenclaw

#endif  // LLM_BACKEND_HH
