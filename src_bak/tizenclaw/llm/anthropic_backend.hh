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
#ifndef ANTHROPIC_BACKEND_HH
#define ANTHROPIC_BACKEND_HH

#include "llm_backend.hh"

namespace tizenclaw {

class AnthropicBackend : public LlmBackend {
 public:
  [[nodiscard]] bool Initialize(const nlohmann::json& config) override;
  [[nodiscard]] LlmResponse Chat(
      const std::vector<LlmMessage>& messages,
      const std::vector<LlmToolDecl>& tools,
      std::function<void(const std::string&)> on_chunk = nullptr,
      const std::string& system_prompt = "") override;
  [[nodiscard]] std::string GetName() const override { return "anthropic"; }

 private:
  nlohmann::json ToAnthropicMessages(
      const std::vector<LlmMessage>& messages) const;
  nlohmann::json ToAnthropicTools(const std::vector<LlmToolDecl>& tools) const;
  LlmResponse ParseAnthropicResponse(const std::string& body) const;

  std::string api_key_;
  std::string model_;
};

}  // namespace tizenclaw

#endif  // ANTHROPIC_BACKEND_HH
