#ifndef __DISCORD_CHANNEL_H__
#define __DISCORD_CHANNEL_H__

#include <string>
#include <vector>
#include <thread>
#include <atomic>
#include <set>
#include <mutex>
#include <json.hpp>

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

    std::string GetName() const override {
      return "discord";
    }
    bool Start() override;
    void Stop() override;
    bool IsRunning() const override {
      return running_;
    }

private:
    bool LoadConfig();

    // Get Gateway URL
    std::string GetGatewayUrl();

    // WebSocket event loop
    void GatewayLoop();

    // Process a message create event
    void HandleMessageCreate(
        const nlohmann::json& data);

    // Send a reply
    void SendReply(
        const std::string& channel_id,
        const std::string& text);

    AgentCore* agent_;
    std::thread ws_thread_;
    std::atomic<bool> running_{false};

    // Config
    std::string bot_token_;
    std::set<std::string> allowed_guilds_;
    std::set<std::string> allowed_channels_;
    int intents_ = 0;  // set to MESSAGE_CONTENT
};

} // namespace tizenclaw

#endif // __DISCORD_CHANNEL_H__
