#ifndef __GEMINI_BACKEND_H__
#define __GEMINI_BACKEND_H__

#include "llm_backend.hh"

namespace tizenclaw {


class GeminiBackend : public LlmBackend {
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
    return "gemini";
  }

private:
  // Convert unified messages to Gemini format
  nlohmann::json ToGeminiContents(
      const std::vector<LlmMessage>& messages) const;
  nlohmann::json ToGeminiTools(
      const std::vector<LlmToolDecl>& tools) const;
  LlmResponse ParseGeminiResponse(
      const std::string& body) const;

  std::string api_key_;
  std::string model_;
};

} // namespace tizenclaw

#endif  // __GEMINI_BACKEND_H__
