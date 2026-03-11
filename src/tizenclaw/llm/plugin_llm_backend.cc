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

#include "plugin_llm_backend.hh"

#include <dlfcn.h>

#include <cstdlib>

#include "../../common/logging.hh"

namespace tizenclaw {

extern "C" {
static void plugin_chunk_callback(const char* chunk, void* user_data) {
  if (!user_data || !chunk) return;
  auto cb = static_cast<std::function<void(const std::string&)>*>(user_data);
  if (*cb) {
    (*cb)(chunk);
  }
}

static bool parse_tool_call_callback(tizenclaw_llm_tool_call_h tool_call,
                                     void* user_data) {
  auto* result = static_cast<LlmResponse*>(user_data);
  char* id = nullptr;
  char* name = nullptr;
  char* args_json = nullptr;

  tizenclaw_llm_tool_call_get_id(tool_call, &id);
  tizenclaw_llm_tool_call_get_name(tool_call, &name);
  tizenclaw_llm_tool_call_get_args_json(tool_call, &args_json);

  LlmToolCall call;
  if (id) {
    call.id = id;
    free(id);
  }
  if (name) {
    call.name = name;
    free(name);
  }
  if (args_json) {
    try {
      call.args = nlohmann::json::parse(args_json);
    } catch (const std::exception& e) {
      LOG(ERROR) << "Failed to parse tool call args JSON: " << e.what();
    }
    free(args_json);
  }
  result->tool_calls.push_back(call);
  return true;  // continue next
}
}

PluginLlmBackend::PluginLlmBackend(const std::string& pkgid,
                                   const std::string& so_path,
                                   const nlohmann::json& config)
    : pkgid_(pkgid), so_path_(so_path), config_(config) {}

PluginLlmBackend::~PluginLlmBackend() { Shutdown(); }

bool PluginLlmBackend::Initialize(const nlohmann::json& config) {
  // Not used directly, but part of LlmBackend interface.
  // We use the internal Initialize() which uses config_ passed in the
  // constructor.
  config_ = config;
  return true;
}

bool PluginLlmBackend::Initialize() {
  dl_handle_ = dlopen(so_path_.c_str(), RTLD_NOW | RTLD_LOCAL);
  if (!dl_handle_) {
    LOG(ERROR) << "dlopen failed for " << so_path_ << ": " << dlerror();
    return false;
  }

  fn_initialize_ = reinterpret_cast<decltype(fn_initialize_)>(
      dlsym(dl_handle_, "TIZENCLAW_LLM_BACKEND_INITIALIZE"));
  fn_get_name_ = reinterpret_cast<decltype(fn_get_name_)>(
      dlsym(dl_handle_, "TIZENCLAW_LLM_BACKEND_GET_NAME"));
  fn_chat_ = reinterpret_cast<decltype(fn_chat_)>(
      dlsym(dl_handle_, "TIZENCLAW_LLM_BACKEND_CHAT"));
  fn_shutdown_ = reinterpret_cast<decltype(fn_shutdown_)>(
      dlsym(dl_handle_, "TIZENCLAW_LLM_BACKEND_SHUTDOWN"));

  if (!fn_initialize_ || !fn_get_name_ || !fn_chat_ || !fn_shutdown_) {
    LOG(ERROR) << "Failed to find required plugin C APIs in " << so_path_;
    dlclose(dl_handle_);
    dl_handle_ = nullptr;
    return false;
  }

  std::string config_str = config_.dump();
  if (!fn_initialize_(config_str.c_str())) {
    LOG(ERROR) << "C API initialization failed for " << so_path_;
    return false;
  }

  // Handle was removed from initialization signature as it's globally managed
  // by the plugin itself per singleton dlopen anyway.
  is_initialized_ = true;

  LOG(INFO) << "Plugin backend loaded and initialized: " << pkgid_;
  return true;
}

void PluginLlmBackend::Shutdown() {
  if (is_initialized_ && fn_shutdown_) {
    fn_shutdown_();
    is_initialized_ = false;
  }
  if (dl_handle_) {
    dlclose(dl_handle_);
    dl_handle_ = nullptr;
  }
}

std::string PluginLlmBackend::GetName() const {
  if (is_initialized_ && fn_get_name_) {
    const char* name = fn_get_name_();
    if (name) return name;
  }
  return "UnknownPlugin";
}

LlmResponse PluginLlmBackend::Chat(
    const std::vector<LlmMessage>& messages,
    const std::vector<LlmToolDecl>& tools,
    std::function<void(const std::string&)> on_chunk,
    const std::string& system_prompt) {
  LlmResponse result;

  if (!is_initialized_ || !fn_chat_) {
    result.success = false;
    result.error_message = "Plugin not initialized or missing C APIs";
    return result;
  }

  tizenclaw_llm_messages_h msgs_h = nullptr;
  tizenclaw_llm_messages_create(&msgs_h);
  for (const auto& m : messages) {
    tizenclaw_llm_message_h msg_h = nullptr;
    tizenclaw_llm_message_create(&msg_h);
    tizenclaw_llm_message_set_role(msg_h, m.role.c_str());
    if (!m.text.empty()) {
      tizenclaw_llm_message_set_text(msg_h, m.text.c_str());
    }

    for (const auto& tc : m.tool_calls) {
      tizenclaw_llm_tool_call_h tc_h = nullptr;
      tizenclaw_llm_tool_call_create(&tc_h);
      tizenclaw_llm_tool_call_set_id(tc_h, tc.id.c_str());
      tizenclaw_llm_tool_call_set_name(tc_h, tc.name.c_str());
      std::string args_str = tc.args.dump();
      tizenclaw_llm_tool_call_set_args_json(tc_h, args_str.c_str());

      tizenclaw_llm_message_add_tool_call(msg_h, tc_h);
      tizenclaw_llm_tool_call_destroy(tc_h);
    }

    if (!m.tool_name.empty()) {
      tizenclaw_llm_message_set_tool_name(msg_h, m.tool_name.c_str());
      tizenclaw_llm_message_set_tool_call_id(msg_h, m.tool_call_id.c_str());
      std::string res_str = m.tool_result.dump();
      tizenclaw_llm_message_set_tool_result_json(msg_h, res_str.c_str());
    }

    tizenclaw_llm_messages_add(msgs_h, msg_h);
    tizenclaw_llm_message_destroy(msg_h);
  }

  tizenclaw_llm_tools_h tools_h = nullptr;
  tizenclaw_llm_tools_create(&tools_h);
  for (const auto& t : tools) {
    tizenclaw_llm_tool_h tool_h = nullptr;
    tizenclaw_llm_tool_create(&tool_h);
    tizenclaw_llm_tool_set_name(tool_h, t.name.c_str());
    tizenclaw_llm_tool_set_description(tool_h, t.description.c_str());
    std::string p_str = t.parameters.dump();
    tizenclaw_llm_tool_set_parameters_json(tool_h, p_str.c_str());

    tizenclaw_llm_tools_add(tools_h, tool_h);
    tizenclaw_llm_tool_destroy(tool_h);
  }

  void* user_data = on_chunk ? &on_chunk : nullptr;
  tizenclaw_llm_response_h resp_h = fn_chat_(
      msgs_h, tools_h, plugin_chunk_callback, user_data, system_prompt.c_str());

  tizenclaw_llm_messages_destroy(msgs_h);
  tizenclaw_llm_tools_destroy(tools_h);

  if (!resp_h) {
    result.error_message = "Plugin returned null response handle";
    return result;
  }

  tizenclaw_llm_response_is_success(resp_h, &result.success);

  char* text = nullptr;
  tizenclaw_llm_response_get_text(resp_h, &text);
  if (text) {
    result.text = text;
    free(text);
  }

  char* error_msg = nullptr;
  tizenclaw_llm_response_get_error_message(resp_h, &error_msg);
  if (error_msg) {
    result.error_message = error_msg;
    free(error_msg);
  }

  // Parse tool calls
  tizenclaw_llm_response_foreach_llm_tool_calls(
      resp_h, parse_tool_call_callback, &result);

  tizenclaw_llm_response_get_prompt_tokens(resp_h, &result.prompt_tokens);
  tizenclaw_llm_response_get_completion_tokens(resp_h,
                                               &result.completion_tokens);
  tizenclaw_llm_response_get_total_tokens(resp_h, &result.total_tokens);
  tizenclaw_llm_response_get_http_status(resp_h, &result.http_status);

  if (!result.success && result.text.empty() && result.error_message.empty()) {
    result.error_message = "Plugin explicitly reported failure without text.";
  }

  tizenclaw_llm_response_destroy(resp_h);

  return result;
}

}  // namespace tizenclaw
