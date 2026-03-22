#ifndef MOCK_LLM_BACKEND_HH_
#define MOCK_LLM_BACKEND_HH_

#include <gmock/gmock.h>
#include "llm_backend.hh"

namespace tizenclaw {

class MockLlmBackend : public LlmBackend {
 public:
  MOCK_METHOD(bool, Initialize,
              (const nlohmann::json& config),
              (override));
  MOCK_METHOD(LlmResponse, Chat,
              (const std::vector<LlmMessage>& messages,
               const std::vector<LlmToolDecl>& tools,
               std::function<void(const std::string&)>
                   on_chunk,
               const std::string& system_prompt),
              (override));
  MOCK_METHOD(std::string, GetName, (),
              (const, override));
  MOCK_METHOD(void, Shutdown, (),
              (override));
};

}  // namespace tizenclaw

#endif  // MOCK_LLM_BACKEND_HH_
