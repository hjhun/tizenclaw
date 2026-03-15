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
#ifndef PLUGIN_CHANNEL_HH
#define PLUGIN_CHANNEL_HH

#include <json.hpp>
#include <string>

#include "channel.hh"
#include "tizenclaw_channel.h"

namespace tizenclaw {

class AgentCore;  // forward declaration

// Wraps a dynamically-loaded channel plugin (.so)
// that exports the TIZENCLAW_CHANNEL_* C API.
// Mirrors the PluginLlmBackend pattern.
class PluginChannel : public Channel {
 public:
  PluginChannel(const std::string& pkgid,
                const std::string& so_path,
                const nlohmann::json& config,
                AgentCore* agent);
  ~PluginChannel() override;

  // Load .so and resolve symbols
  [[nodiscard]] bool Initialize();

  // Channel interface
  [[nodiscard]] std::string GetName() const override;
  [[nodiscard]] bool Start() override;
  void Stop() override;
  [[nodiscard]] bool IsRunning() const override {
    return running_;
  }
  bool SendMessage(
      const std::string& text) override;

  const std::string& GetPkgId() const { return pkgid_; }

 private:
  std::string pkgid_;
  std::string so_path_;
  nlohmann::json config_;
  AgentCore* agent_;

  void* dl_handle_ = nullptr;
  bool running_ = false;

  // Function pointers from plugin .so
  decltype(TIZENCLAW_CHANNEL_INITIALIZE)* fn_initialize_ =
      nullptr;
  decltype(TIZENCLAW_CHANNEL_GET_NAME)* fn_get_name_ =
      nullptr;
  decltype(TIZENCLAW_CHANNEL_START)* fn_start_ = nullptr;
  decltype(TIZENCLAW_CHANNEL_STOP)* fn_stop_ = nullptr;
  decltype(TIZENCLAW_CHANNEL_SEND_MESSAGE)*
      fn_send_message_ = nullptr;  // optional
};

}  // namespace tizenclaw

#endif  // PLUGIN_CHANNEL_HH
