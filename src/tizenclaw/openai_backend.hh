#ifndef __OPENAI_BACKEND_H__
#define __OPENAI_BACKEND_H__

#include "llm_backend.hh"

namespace tizenclaw {


// Shared by OpenAI (ChatGPT) and xAI (Grok)
// since xAI uses OpenAI-compatible API.
class OpenAiBackend : public LlmBackend {
public:
  bool Initialize(
      const nlohmann::json& config) override;
  LlmResponse Chat(
      const std::vector<LlmMessage>& messages,
      const std::vector<LlmToolDecl>& tools,
      std::function<void(const std::string&)> on_chunk = nullptr,
      const std::string& system_prompt = "")
      override;
  std::string GetName() const override {
    return name_;
  }

private:
  nlohmann::json ToOpenAiMessages(
      const std::vector<LlmMessage>& messages) const;
  nlohmann::json ToOpenAiTools(
      const std::vector<LlmToolDecl>& tools) const;
  LlmResponse ParseOpenAiResponse(
      const std::string& body) const;

  std::string api_key_;
  std::string model_;
  std::string endpoint_;
  std::string name_ = "openai";
};

} // namespace tizenclaw

#endif  // __OPENAI_BACKEND_H__
