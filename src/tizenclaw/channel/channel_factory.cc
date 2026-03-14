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
#include "channel_factory.hh"

#include <fstream>
#include <nlohmann/json.hpp>

#include "../../common/logging.hh"
#include "discord_channel.hh"
#include "mcp_server.hh"
#include "slack_channel.hh"
#include "telegram_client.hh"
#include "voice_channel.hh"
#include "web_dashboard.hh"
#include "webhook_channel.hh"

namespace tizenclaw {

namespace {

constexpr char kConfigDir[] =
    "/opt/usr/share/tizenclaw/config/";

}  // namespace

bool ChannelFactory::IsExternalConfigValid(
    const std::string& config_file,
    const std::vector<std::string>& required_keys) {
  std::string path =
      std::string(kConfigDir) + config_file;
  std::ifstream f(path);
  if (!f.is_open()) return false;
  try {
    nlohmann::json j;
    f >> j;

    // Special case: webhook routes
    if (config_file == "webhook_config.json") {
      return j.contains("routes") &&
             j["routes"].is_array() &&
             !j["routes"].empty();
    }

    for (const auto& key : required_keys) {
      if (!j.contains(key)) return false;
      if (j[key].is_string()) {
        std::string val = j[key].get<std::string>();
        if (val.empty() ||
            val.find("YOUR_") != std::string::npos)
          return false;
      }
    }
    return true;
  } catch (...) {
    return false;
  }
}

void ChannelFactory::CreateFromConfig(
    const std::string& config_path,
    AgentCore* agent,
    TaskScheduler* scheduler,
    ChannelRegistry& registry) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "channels.json not found at "
                 << config_path
                 << ", using default channels";
    // Fallback: register core channels
    registry.Register(
        std::make_unique<McpServer>(agent));
    registry.Register(
        std::make_unique<WebDashboard>(
            agent, scheduler));
    return;
  }

  nlohmann::json cfg;
  try {
    f >> cfg;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse channels.json: "
               << e.what();
    registry.Register(
        std::make_unique<McpServer>(agent));
    registry.Register(
        std::make_unique<WebDashboard>(
            agent, scheduler));
    return;
  }

  if (!cfg.contains("channels") ||
      !cfg["channels"].is_array()) {
    LOG(WARNING) << "channels.json: missing "
                 << "'channels' array";
    return;
  }

  for (const auto& ch : cfg["channels"]) {
    std::string name = ch.value("name", "");
    bool enabled = ch.value("enabled", false);
    if (name.empty() || !enabled) {
      LOG(INFO) << "Channel skipped (disabled): "
                << name;
      continue;
    }

    // Check external config if specified
    if (ch.contains("config_file")) {
      std::string config_file =
          ch.value("config_file", "");
      std::vector<std::string> keys;
      if (ch.contains("required_keys")) {
        for (const auto& k : ch["required_keys"])
          keys.push_back(k.get<std::string>());
      }
      if (!config_file.empty() &&
          !IsExternalConfigValid(config_file, keys)) {
        LOG(INFO) << "Channel " << name
                  << ": config not ready ("
                  << config_file << "), skipping";
        continue;
      }
    }

    // Create built-in channel by name
    if (name == "mcp") {
      registry.Register(
          std::make_unique<McpServer>(agent));
    } else if (name == "web_dashboard") {
      registry.Register(
          std::make_unique<WebDashboard>(
              agent, scheduler));
    } else if (name == "telegram") {
      registry.Register(
          std::make_unique<TelegramClient>(agent));
    } else if (name == "webhook") {
      registry.Register(
          std::make_unique<WebhookChannel>(agent));
    } else if (name == "slack") {
      registry.Register(
          std::make_unique<SlackChannel>(agent));
    } else if (name == "discord") {
      registry.Register(
          std::make_unique<DiscordChannel>(agent));
    } else if (name == "voice") {
      registry.Register(
          std::make_unique<VoiceChannel>(agent));
    } else {
      LOG(WARNING) << "Unknown built-in channel: "
                   << name;
    }

    LOG(INFO) << "Channel registered via config: "
              << name;
  }
}

}  // namespace tizenclaw
