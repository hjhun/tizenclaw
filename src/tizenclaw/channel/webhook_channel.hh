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
#ifndef WEBHOOK_CHANNEL_HH
#define WEBHOOK_CHANNEL_HH

#include <libsoup/soup.h>

#include <atomic>
#include <string>
#include <thread>
#include <vector>

#include "channel.hh"

namespace tizenclaw {

class AgentCore;

// Webhook route: maps a URL path to a session
struct WebhookRoute {
  std::string path;
  std::string session_id;
};

// Webhook inbound trigger channel.
// Runs an HTTP server (libsoup) that accepts
// incoming webhook requests, validates HMAC
// signatures, and routes payloads to AgentCore.
class WebhookChannel : public Channel {
 public:
  explicit WebhookChannel(AgentCore* agent);
  ~WebhookChannel();

  // Channel interface
  std::string GetName() const override { return "webhook"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }

  // Public for testing: HMAC verification
  static bool VerifyHmac(const std::string& secret, const std::string& payload,
                         const std::string& signature);

 private:
  // Load webhook_config.json
  bool LoadConfig();

  // libsoup request handler callback
  static void HandleRequest(SoupServer* server, SoupMessage* msg,
                            const char* path, GHashTable* query,
                            SoupClientContext* client, gpointer user_data);

  // Process a webhook request (runs on thread)
  void ProcessWebhook(SoupMessage* msg, const std::string& path,
                      const std::string& body);

  AgentCore* agent_;
  SoupServer* server_ = nullptr;
  std::thread server_thread_;
  GMainContext* context_ = nullptr;
  GMainLoop* loop_ = nullptr;
  std::atomic<bool> running_{false};

  // Configuration
  int port_ = 8080;
  std::string hmac_secret_;
  std::vector<WebhookRoute> routes_;
};

}  // namespace tizenclaw

#endif  // WEBHOOK_CHANNEL_HH
