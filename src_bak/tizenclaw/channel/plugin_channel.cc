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
#include "plugin_channel.hh"

#include <dlfcn.h>

#include <cstdlib>
#include <cstring>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"

namespace tizenclaw {

namespace {

// C callback bridging plugin → AgentCore::ProcessPrompt.
// Returns a malloc'd string the plugin must free().
char* PluginPromptCallback(const char* session_id,
                           const char* text,
                           void* user_data) {
  if (!user_data || !text) return nullptr;
  auto* agent = static_cast<AgentCore*>(user_data);
  std::string sid = session_id ? session_id : "default";
  std::string result = agent->ProcessPrompt(sid, text);
  return strdup(result.c_str());
}

}  // namespace

PluginChannel::PluginChannel(const std::string& pkgid,
                             const std::string& so_path,
                             const nlohmann::json& config,
                             AgentCore* agent)
    : pkgid_(pkgid),
      so_path_(so_path),
      config_(config),
      agent_(agent) {}

PluginChannel::~PluginChannel() {
  Stop();
  if (dl_handle_) {
    dlclose(dl_handle_);
    dl_handle_ = nullptr;
  }
}

bool PluginChannel::Initialize() {
  dl_handle_ = dlopen(so_path_.c_str(),
                       RTLD_NOW | RTLD_LOCAL);
  if (!dl_handle_) {
    LOG(ERROR) << "dlopen failed for " << so_path_
               << ": " << dlerror();
    return false;
  }

  fn_initialize_ =
      reinterpret_cast<decltype(fn_initialize_)>(
          dlsym(dl_handle_,
                "TIZENCLAW_CHANNEL_INITIALIZE"));
  fn_get_name_ =
      reinterpret_cast<decltype(fn_get_name_)>(
          dlsym(dl_handle_,
                "TIZENCLAW_CHANNEL_GET_NAME"));
  fn_start_ =
      reinterpret_cast<decltype(fn_start_)>(
          dlsym(dl_handle_,
                "TIZENCLAW_CHANNEL_START"));
  fn_stop_ =
      reinterpret_cast<decltype(fn_stop_)>(
          dlsym(dl_handle_,
                "TIZENCLAW_CHANNEL_STOP"));

  // Optional: outbound messaging
  fn_send_message_ =
      reinterpret_cast<decltype(fn_send_message_)>(
          dlsym(dl_handle_,
                "TIZENCLAW_CHANNEL_SEND_MESSAGE"));

  if (!fn_initialize_ || !fn_get_name_ ||
      !fn_start_ || !fn_stop_) {
    LOG(ERROR) << "Missing TIZENCLAW_CHANNEL_* "
               << "symbols in " << so_path_;
    dlclose(dl_handle_);
    dl_handle_ = nullptr;
    return false;
  }

  std::string cfg = config_.dump();
  if (!fn_initialize_(cfg.c_str())) {
    LOG(ERROR) << "Channel plugin init failed: "
               << so_path_;
    return false;
  }

  LOG(INFO) << "Channel plugin loaded: " << pkgid_
            << " (" << GetName() << ")";
  return true;
}

std::string PluginChannel::GetName() const {
  if (fn_get_name_) {
    const char* name = fn_get_name_();
    if (name) return name;
  }
  return "unknown_plugin_channel";
}

bool PluginChannel::Start() {
  if (running_) return true;
  if (!fn_start_) return false;

  bool ok = fn_start_(PluginPromptCallback, agent_);
  if (ok) running_ = true;
  return ok;
}

void PluginChannel::Stop() {
  if (!running_) return;
  if (fn_stop_) fn_stop_();
  running_ = false;
}

bool PluginChannel::SendMessage(
    const std::string& text) {
  if (!running_ || !fn_send_message_) return false;
  return fn_send_message_(text.c_str());
}

}  // namespace tizenclaw
