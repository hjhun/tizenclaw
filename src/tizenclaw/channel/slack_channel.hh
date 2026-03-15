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
#ifndef SLACK_CHANNEL_HH
#define SLACK_CHANNEL_HH

#include <atomic>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include "channel.hh"

namespace tizenclaw {

class AgentCore;

// Slack Bot channel using Socket Mode.
//
// Flow:
//  1. POST apps.connections.open (app_token)
//     → get wss:// URL
//  2. Connect via libwebsockets
//  3. Receive events_api envelope
//  4. Acknowledge envelope_id
//  5. Extract message text, call AgentCore
//  6. POST chat.postMessage (bot_token)
class SlackChannel : public Channel {
 public:
  explicit SlackChannel(AgentCore* agent);
  ~SlackChannel();

  std::string GetName() const override { return "slack"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }
  bool SendMessage(const std::string& text) override;

 private:
  bool LoadConfig();

  // Get WebSocket URL via apps.connections.open
  std::string GetSocketModeUrl();

  // WebSocket event loop
  void SocketLoop();

  // Process a Slack message event
  void HandleMessageEvent(const std::string& channel, const std::string& user,
                          const std::string& text, const std::string& ts);

  // Send a reply via chat.postMessage
  void SendReply(const std::string& channel, const std::string& text,
                 const std::string& thread_ts = "");

  AgentCore* agent_;
  std::thread ws_thread_;
  std::atomic<bool> running_{false};

  // Config
  std::string bot_token_;
  std::string app_token_;
  std::set<std::string> allowed_channels_;
};

}  // namespace tizenclaw

#endif  // SLACK_CHANNEL_HH
