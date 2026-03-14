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
#ifndef CHANNEL_FACTORY_HH
#define CHANNEL_FACTORY_HH

#include <string>

#include "channel_registry.hh"

namespace tizenclaw {

class AgentCore;
class TaskScheduler;

// Creates and registers built-in channels based on a
// JSON configuration file (channels.json).
// Replaces the hardcoded channel registration that was
// previously in TizenClawDaemon::OnCreate().
class ChannelFactory {
 public:
  // Load channels.json → register enabled channels.
  // Channels whose external config is missing or
  // incomplete are silently skipped (non-fatal).
  static void CreateFromConfig(
      const std::string& config_path,
      AgentCore* agent,
      TaskScheduler* scheduler,
      ChannelRegistry& registry);

 private:
  // Check if a channel's external config file is
  // present and contains all required keys.
  static bool IsExternalConfigValid(
      const std::string& config_file,
      const std::vector<std::string>& required_keys);
};

}  // namespace tizenclaw

#endif  // CHANNEL_FACTORY_HH
