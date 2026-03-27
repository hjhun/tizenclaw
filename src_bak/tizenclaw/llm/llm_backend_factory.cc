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
#include <functional>
#include <unordered_map>

#include "../../common/logging.hh"
#include "anthropic_backend.hh"
#include "gemini_backend.hh"
#include "llm_backend.hh"
#include "ollama_backend.hh"
#include "openai_backend.hh"
#include "plugin_llm_backend.hh"
#include "plugin_manager.hh"

namespace tizenclaw {

// Registry Map — GoF Factory Pattern (OCP)
// Adding a new backend requires only one
// additional line in this map.
using BackendCreator = std::function<std::unique_ptr<LlmBackend>()>;

static const std::unordered_map<std::string, BackendCreator> kBackendRegistry =
    {
        {"gemini", [] { return std::make_unique<GeminiBackend>(); }},
        {"openai", [] { return std::make_unique<OpenAiBackend>(); }},
        {"chatgpt", [] { return std::make_unique<OpenAiBackend>(); }},
        {"xai", [] { return std::make_unique<OpenAiBackend>(); }},
        {"grok", [] { return std::make_unique<OpenAiBackend>(); }},
        {"anthropic", [] { return std::make_unique<AnthropicBackend>(); }},
        {"claude", [] { return std::make_unique<AnthropicBackend>(); }},
        {"ollama", [] { return std::make_unique<OllamaBackend>(); }},
};

class PluginAdapter : public LlmBackend {
 public:
  PluginAdapter(std::shared_ptr<PluginLlmBackend> backend)
      : backend_(backend) {}
  bool Initialize(const nlohmann::json& config) override {
    return backend_->Initialize(config);
  }
  LlmResponse Chat(const std::vector<LlmMessage>& messages,
                   const std::vector<LlmToolDecl>& tools,
                   std::function<void(const std::string&)> on_chunk,
                   const std::string& system_prompt) override {
    return backend_->Chat(messages, tools, on_chunk, system_prompt);
  }
  std::string GetName() const override { return backend_->GetName(); }
  void Shutdown() override {}

 private:
  std::shared_ptr<PluginLlmBackend> backend_;
};

std::unique_ptr<LlmBackend> LlmBackendFactory::Create(const std::string& name) {
  // Check plugins first so they can override built-in backends
  auto plugins = PluginManager::GetInstance().GetLlmBackends();
  for (auto& p : plugins) {
    if (p->GetName() == name) {
      return std::make_unique<PluginAdapter>(p);
    }
  }

  if (auto it = kBackendRegistry.find(name); it != kBackendRegistry.end()) {
    return it->second();
  }

  LOG(ERROR) << "Unknown LLM backend: " << name;
  return nullptr;
}

}  // namespace tizenclaw
