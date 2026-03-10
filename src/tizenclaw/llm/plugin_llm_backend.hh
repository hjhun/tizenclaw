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

#ifndef TIZENCLAW_PLUGIN_LLM_BACKEND_HH_
#define TIZENCLAW_PLUGIN_LLM_BACKEND_HH_

#include <json.hpp>
#include <string>

#include "llm_backend.hh"
#include "tizenclaw_llm_backend.h"

namespace tizenclaw {

class PluginLlmBackend : public LlmBackend {
 public:
  PluginLlmBackend(const std::string& pkgid, const std::string& so_path,
                   const nlohmann::json& config);
  virtual ~PluginLlmBackend() override;

  bool Initialize();  // Internal init
  bool Initialize(const nlohmann::json& config) override;
  void Shutdown() override;

  std::string GetName() const override;

  // Expose configuration (priority, etc.)
  const nlohmann::json& GetConfig() const { return config_; }
  const std::string& GetPkgId() const { return pkgid_; }

  LlmResponse Chat(const std::vector<LlmMessage>& messages,
                   const std::vector<LlmToolDecl>& tools,
                   std::function<void(const std::string&)> on_chunk = nullptr,
                   const std::string& system_prompt = "") override;

 private:
  std::string pkgid_;
  std::string so_path_;
  nlohmann::json config_;

  void* dl_handle_ = nullptr;
  bool is_initialized_ = false;

  // Function pointers mapped from plugin shared library
  decltype(TIZENCLAW_LLM_BACKEND_INITIALIZE)* fn_initialize_ = nullptr;
  decltype(TIZENCLAW_LLM_BACKEND_GET_NAME)* fn_get_name_ = nullptr;
  decltype(TIZENCLAW_LLM_BACKEND_CHAT)* fn_chat_ = nullptr;
  decltype(TIZENCLAW_LLM_BACKEND_SHUTDOWN)* fn_shutdown_ = nullptr;
};

}  // namespace tizenclaw

#endif  // TIZENCLAW_PLUGIN_LLM_BACKEND_HH_
