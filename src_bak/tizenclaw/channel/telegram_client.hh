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
#ifndef TELEGRAM_CLIENT_HH
#define TELEGRAM_CLIENT_HH

#include <atomic>
#include <set>
#include <string>
#include <thread>

#include "channel.hh"

namespace tizenclaw {

// Forward declaration
class AgentCore;

class TelegramClient : public Channel {
 public:
  explicit TelegramClient(AgentCore* agent);
  ~TelegramClient();

  // Channel interface
  std::string GetName() const override { return "telegram"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }
  bool SendMessage(const std::string& text) override;

 private:
  // Main loop for fetching updates using long-polling
  void PollingLoop();

  // Async handler: processes a single message on a worker thread
  void HandleMessage(long chat_id, const std::string& text);

  // Parses telegram_config.json
  bool LoadConfig();

  // Sends a message back to the user via Telegram API
  void SendMessage(long chat_id, const std::string& text);

  // Edits an existing message (returns true on success)
  bool EditMessage(long chat_id, long message_id, const std::string& text);

  AgentCore* agent_;
  std::string bot_token_;
  std::set<long> allowed_chat_ids_;

  std::thread polling_thread_;
  std::atomic<bool> running_;
  long update_offset_ = 0;

  // Concurrency control for message handlers
  std::atomic<int> active_handlers_{0};
  static constexpr int kMaxConcurrentHandlers = 3;

  // Exponential backoff for HTTP 409 conflicts
  int conflict_backoff_sec_ = 5;
};

}  // namespace tizenclaw

#endif  // TELEGRAM_CLIENT_HH
