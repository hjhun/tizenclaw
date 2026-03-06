#ifndef __LLM_BACKEND_H__
#define __LLM_BACKEND_H__

#include <memory>
#include <string>
#include <vector>
#include <json.hpp>
#include <functional>

namespace tizenclaw {


// --------------------------------------------------
// Unified message structures (provider-agnostic)
// --------------------------------------------------

struct LlmToolCall {
  std::string id;       // provider-assigned ID (e.g. "call_abc123")
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

  bool HasToolCalls() const {
    return !tool_calls.empty();
  }
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
  virtual bool Initialize(
      const nlohmann::json& config) = 0;

  // Send a chat request. Returns unified response.
  virtual LlmResponse Chat(
      const std::vector<LlmMessage>& messages,
      const std::vector<LlmToolDecl>& tools,
      std::function<void(const std::string&)> on_chunk = nullptr,
      const std::string& system_prompt = "") = 0;

  // Provider name (e.g. "gemini", "openai")
  virtual std::string GetName() const = 0;

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
  static std::unique_ptr<LlmBackend> Create(
      const std::string& name);
};

} // namespace tizenclaw

#endif  // __LLM_BACKEND_H__
