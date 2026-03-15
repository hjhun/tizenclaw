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
#ifndef DISCORD_CHANNEL_HH
#define DISCORD_CHANNEL_HH

#include <atomic>
#include <json.hpp>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include "channel.hh"

namespace tizenclaw {

class AgentCore;

// Discord Bot channel using Gateway WebSocket.
//
// Flow:
//  1. GET /gateway/bot → wss:// URL
//  2. Connect via libwebsockets
//  3. Receive Hello → start heartbeat
//  4. Send Identify
//  5. Receive MESSAGE_CREATE events
//  6. POST /channels/{id}/messages
class DiscordChannel : public Channel {
 public:
  explicit DiscordChannel(AgentCore* agent);
  ~DiscordChannel();

  std::string GetName() const override { return "discord"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }
  bool SendMessage(const std::string& text) override;

 private:
  bool LoadConfig();

  // Get Gateway URL
  std::string GetGatewayUrl();

  // WebSocket event loop
  void GatewayLoop();

  // Process a message create event
  void HandleMessageCreate(const nlohmann::json& data);

  // Send a reply
  void SendReply(const std::string& channel_id, const std::string& text);

  AgentCore* agent_;
  std::thread ws_thread_;
  std::atomic<bool> running_{false};

  // Config
  std::string bot_token_;
  std::set<std::string> allowed_guilds_;
  std::set<std::string> allowed_channels_;
  int intents_ = 0;  // set to MESSAGE_CONTENT
};

}  // namespace tizenclaw

#endif  // DISCORD_CHANNEL_HH
