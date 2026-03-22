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
#include "telegram_client.hh"

#include <chrono>
#include <fstream>
#include <iostream>
#include <mutex>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"
#include "../infra/http_client.hh"

namespace tizenclaw {

TelegramClient::TelegramClient(AgentCore* agent)
    : agent_(agent), running_(false) {}

TelegramClient::~TelegramClient() { Stop(); }

bool TelegramClient::LoadConfig() {
  std::string config_path =
      "/opt/usr/share/tizenclaw/config/"
      "telegram_config.json";
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "No telegram_config.json found";
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;
    bot_token_ = j.value("bot_token", "");

    if (j.contains("allowed_chat_ids") && j["allowed_chat_ids"].is_array()) {
      for (auto& id : j["allowed_chat_ids"]) {
        allowed_chat_ids_.insert(id.get<long>());
      }
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse config: " << e.what();
    return false;
  }

  if (bot_token_.empty() || bot_token_ == "YOUR_TELEGRAM_BOT_TOKEN_HERE") {
    LOG(WARNING) << "Invalid or default BOT_TOKEN.";
    return false;
  }

  return true;
}

bool TelegramClient::Start() {
  if (running_) {
    return true;
  }

  if (!LoadConfig()) {
    return false;
  }

  // Clear any prior webhook or polling session
  // to prevent HTTP 409 Conflict errors.
  std::string reset_url =
      "https://api.telegram.org/bot" +
      bot_token_ + "/deleteWebhook";
  auto reset_resp = HttpClient::Get(reset_url);
  if (reset_resp.success) {
    LOG(INFO) << "TelegramClient: cleared prior "
              << "webhook/polling session";
  } else {
    LOG(WARNING) << "TelegramClient: "
                 << "deleteWebhook failed: "
                 << reset_resp.error;
  }

  running_ = true;
  polling_thread_ = std::thread(
      &TelegramClient::PollingLoop, this);
  LOG(INFO) << "TelegramClient started polling.";
  return true;
}

void TelegramClient::Stop() {
  if (running_) {
    running_ = false;
    if (polling_thread_.joinable()) {
      polling_thread_.join();
    }
    LOG(INFO) << "TelegramClient stopped.";
  }
}

bool TelegramClient::SendMessage(
    const std::string& text) {
  if (!running_ || bot_token_.empty()) return false;
  if (allowed_chat_ids_.empty()) {
    LOG(WARNING) << "Telegram: no chat_ids for "
                 << "outbound message";
    return false;
  }
  for (long chat_id : allowed_chat_ids_) {
    SendMessage(chat_id, text);
  }
  return true;
}

void TelegramClient::SendMessage(long chat_id, const std::string& text) {
  if (bot_token_.empty()) return;

  std::string url =
      "https://api.telegram.org/bot" + bot_token_ + "/sendMessage";

  // Truncate to Telegram's 4096 char limit
  std::string safe_text = text;
  if (safe_text.length() > 4000) {
    safe_text = safe_text.substr(0, 4000) + "\n...(truncated)";
  }

  nlohmann::json payload = {
      {"chat_id", chat_id}, {"text", safe_text}, {"parse_mode", "Markdown"}};

  auto resp = HttpClient::Post(url, {{"Content-Type", "application/json"}},
                               payload.dump());

  if (!resp.success) {
    LOG(WARNING) << "SendMessage Markdown parse "
                 << "failed, retrying plain text";
    payload.erase("parse_mode");
    resp = HttpClient::Post(url, {{"Content-Type", "application/json"}},
                            payload.dump());

    if (!resp.success) {
      LOG(ERROR) << "SendMessage failed: " << resp.error;
    }
  }
}

bool TelegramClient::EditMessage(long chat_id, long message_id,
                                 const std::string& text) {
  if (bot_token_.empty()) return false;

  std::string url =
      "https://api.telegram.org/bot" + bot_token_ + "/editMessageText";

  std::string safe_text = text;
  if (safe_text.length() > 4000) {
    safe_text = safe_text.substr(0, 4000) + "\n...(truncated)";
  }
  if (safe_text.empty()) {
    safe_text = "...";
  }

  nlohmann::json payload = {
      {"chat_id", chat_id}, {"message_id", message_id}, {"text", safe_text}};

  auto resp = HttpClient::Post(url, {{"Content-Type", "application/json"}},
                               payload.dump(), 1, 5, 10);

  return resp.success;
}

void TelegramClient::PollingLoop() {
  std::string url = "https://api.telegram.org/bot" + bot_token_;

  while (running_) {
    std::string req_url =
        url + "/getUpdates?offset=" + std::to_string(update_offset_) +
        "&timeout=30";

    // Call HTTP GET with a 40 second timeout
    // (to allow for 30s long polling + network)
    auto resp = HttpClient::Get(req_url, {}, 1, 10, 40);

    if (!running_) break;

    if (!resp.success) {
      // Check for HTTP 409 Conflict — another
      // instance is polling. Apply exponential
      // backoff to avoid log flooding.
      if (resp.error.find("409") !=
          std::string::npos) {
        LOG(WARNING)
            << "Telegram polling conflict "
            << "(HTTP 409): another instance "
            << "may be active. Backing off "
            << conflict_backoff_sec_ << "s";
        std::this_thread::sleep_for(
            std::chrono::seconds(
                conflict_backoff_sec_));
        conflict_backoff_sec_ =
            std::min(conflict_backoff_sec_ * 2,
                     60);
      } else {
        LOG(ERROR) << "Polling network error: "
                   << resp.error;
        conflict_backoff_sec_ = 5;  // reset
        std::this_thread::sleep_for(
            std::chrono::seconds(5));
      }
      continue;
    }

    // Reset backoff on successful poll
    conflict_backoff_sec_ = 5;

    try {
      auto j = nlohmann::json::parse(resp.body);
      if (!j.value("ok", false)) {
        LOG(ERROR) << "API returned not ok";
        std::this_thread::sleep_for(std::chrono::seconds(5));
        continue;
      }

      for (auto& item : j["result"]) {
        update_offset_ = item["update_id"].get<long>() + 1;

        if (!item.contains("message")) {
          continue;
        }

        auto msg = item["message"];
        if (!msg.contains("text") || !msg.contains("chat")) {
          continue;
        }

        std::string text = msg.value("text", "");
        long chat_id = msg["chat"].value("id", 0L);

        if (text.empty() || chat_id == 0) {
          continue;
        }

        // Apply allowlist filter
        if (!allowed_chat_ids_.empty() &&
            allowed_chat_ids_.find(chat_id) == allowed_chat_ids_.end()) {
          LOG(INFO) << "Blocked chat_id " << chat_id << " - not in allowlist";
          continue;
        }

        LOG(INFO) << "Received from " << chat_id << ": " << text;

        // Check concurrent handler limit
        if (active_handlers_.load() >= kMaxConcurrentHandlers) {
          LOG(WARNING) << "Max handlers reached, sending busy reply";
          SendMessage(chat_id,
                      "\u26a0\ufe0f Busy processing other "
                      "requests. Please try again "
                      "shortly.");
          continue;
        }

        // Dispatch to worker thread
        // (non-blocking — polling loop
        //  continues immediately)
        active_handlers_.fetch_add(1);
        std::thread([this, chat_id, text]() {
          HandleMessage(chat_id, text);
          active_handlers_.fetch_sub(1);
        }).detach();
      }
    } catch (const std::exception& e) {
      LOG(ERROR) << "Polling JSON error: " << e.what();
      std::this_thread::sleep_for(std::chrono::seconds(5));
    }
  }
}

void TelegramClient::HandleMessage(long chat_id, const std::string& text) {
  // Send initial placeholder
  std::string url_send =
      "https://api.telegram.org/bot" + bot_token_ + "/sendMessage";
  nlohmann::json init_payload = {{"chat_id", chat_id},
                                 {"text", "\u23f3 Thinking..."}};
  auto init_resp = HttpClient::Post(
      url_send, {{"Content-Type", "application/json"}}, init_payload.dump());

  long msg_id = 0;
  if (init_resp.success) {
    try {
      auto jr = nlohmann::json ::parse(init_resp.body);
      msg_id = jr["result"].value("message_id", 0L);
    } catch (...) {
    }
  }

  // Streaming callback with throttled edits
  std::string accumulated;
  std::mutex acc_mutex;
  auto last_edit = std::chrono::steady_clock::now();

  auto on_chunk = [&](const std::string& chunk) {
    if (msg_id == 0) return;
    std::lock_guard<std::mutex> lock(acc_mutex);
    accumulated += chunk;
    auto now = std::chrono::steady_clock ::now();
    auto elapsed =
        std::chrono::duration_cast<std::chrono::milliseconds>(now - last_edit)
            .count();
    // 2s throttle — reduces Telegram API
    // rate-limit pressure (429 errors)
    if (elapsed >= 2000) {
      EditMessage(chat_id, msg_id, accumulated);
      last_edit = now;
    }
  };

  std::string session_id = "telegram_" + std::to_string(chat_id);
  std::string response = agent_->ProcessPrompt(session_id, text, on_chunk);

  // Final update with complete response
  if (msg_id > 0) {
    EditMessage(chat_id, msg_id, response);
  } else {
    SendMessage(chat_id, response);
  }
}

}  // namespace tizenclaw
