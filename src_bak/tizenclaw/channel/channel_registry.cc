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
#include "channel_registry.hh"

#include <algorithm>
#include <ranges>

#include "../../common/logging.hh"

namespace tizenclaw {

ChannelRegistry::~ChannelRegistry() { StopAll(); }

void ChannelRegistry::Register(std::unique_ptr<Channel> ch) {
  if (!ch) return;
  LOG(INFO) << "Channel registered: " << ch->GetName();
  channels_.push_back(std::move(ch));
}

void ChannelRegistry::StartAll() {
  for (auto& ch : channels_) {
    if (!ch->Start()) {
      LOG(WARNING) << "Channel failed to start: " << ch->GetName()
                   << " (continuing without it)";
    } else {
      LOG(INFO) << "Channel started: " << ch->GetName();
    }
  }
}

void ChannelRegistry::StopAll() {
  // Stop in reverse registration order
  for (auto& ch : channels_ | std::views::reverse) {
    if (ch->IsRunning()) {
      LOG(INFO) << "Stopping channel: " << ch->GetName();
      ch->Stop();
    }
  }
}

Channel* ChannelRegistry::Get(const std::string& name) const {
  auto it = std::ranges::find_if(
      channels_, [&name](const auto& ch) { return ch->GetName() == name; });
  return it != channels_.end() ? it->get() : nullptr;
}

std::vector<std::string> ChannelRegistry::ListChannels() const {
  std::vector<std::string> names;
  names.reserve(channels_.size());
  std::ranges::transform(channels_, std::back_inserter(names),
                         [](const auto& ch) { return ch->GetName(); });
  return names;
}

bool ChannelRegistry::SendTo(
    const std::string& channel_name,
    const std::string& text) {
  auto* ch = Get(channel_name);
  if (!ch || !ch->IsRunning()) return false;
  return ch->SendMessage(text);
}

void ChannelRegistry::Broadcast(
    const std::string& text) {
  for (auto& ch : channels_) {
    if (ch->IsRunning()) {
      if (ch->SendMessage(text)) {
        LOG(INFO) << "Broadcast sent to: "
                  << ch->GetName();
      }
    }
  }
}

}  // namespace tizenclaw
