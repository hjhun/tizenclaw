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
#ifndef CHANNEL_HH
#define CHANNEL_HH

#include <string>

namespace tizenclaw {

// Abstract interface for communication channels.
// Each channel (Telegram, MCP, future Slack/Discord)
// implements this interface and registers with the
// ChannelRegistry for lifecycle management.
class Channel {
 public:
  virtual ~Channel() = default;

  // Human-readable channel name (e.g. "telegram")
  [[nodiscard]] virtual std::string GetName() const = 0;

  // Initialize and start the channel.
  // Returns false if startup fails (e.g. missing
  // config). Non-fatal: daemon continues without
  // this channel.
  [[nodiscard]] virtual bool Start() = 0;

  // Signal the channel to stop and clean up.
  virtual void Stop() = 0;

  // Whether the channel is currently active.
  [[nodiscard]] virtual bool IsRunning() const = 0;

  // Send a proactive outbound message through this
  // channel (e.g. LLM-initiated notification).
  // Returns true if the message was delivered.
  // Default: false (channel does not support
  // outbound push).
  virtual bool SendMessage(
      const std::string& /*text*/) {
    return false;
  }
};

}  // namespace tizenclaw

#endif  // CHANNEL_HH
