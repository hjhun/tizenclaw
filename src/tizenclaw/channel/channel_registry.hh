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
#ifndef CHANNEL_REGISTRY_HH
#define CHANNEL_REGISTRY_HH

#include <memory>
#include <string>
#include <vector>

#include "channel.hh"

namespace tizenclaw {

// Manages the lifecycle of all registered channels.
// Channels are registered during daemon startup and
// started/stopped as a group.
class ChannelRegistry {
 public:
  ChannelRegistry() = default;
  ~ChannelRegistry();

  // Takes ownership of a channel.
  void Register(std::unique_ptr<Channel> ch);

  // Start all registered channels.
  // Channels that fail to start are logged but
  // do not prevent other channels from starting.
  void StartAll();

  // Stop all running channels in reverse order.
  void StopAll();

  // Look up a channel by name (nullptr if not
  // found).
  [[nodiscard]] Channel* Get(const std::string& name) const;

  // List names of all registered channels.
  [[nodiscard]] std::vector<std::string> ListChannels() const;

  // Send a message to a specific channel by name.
  // Returns false if channel not found or send
  // not supported.
  bool SendTo(const std::string& channel_name,
              const std::string& text);

  // Broadcast a message to all running channels
  // that support outbound messaging.
  void Broadcast(const std::string& text);

  // Number of registered channels.
  [[nodiscard]] size_t Size() const { return channels_.size(); }

 private:
  std::vector<std::unique_ptr<Channel>> channels_;
};

}  // namespace tizenclaw

#endif  // CHANNEL_REGISTRY_HH
