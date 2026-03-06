#ifndef __WEBHOOK_CHANNEL_H__
#define __WEBHOOK_CHANNEL_H__

#include <string>
#include <vector>
#include <thread>
#include <atomic>
#include <libsoup/soup.h>

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
    std::string GetName() const override {
      return "webhook";
    }
    bool Start() override;
    void Stop() override;
    bool IsRunning() const override {
      return running_;
    }

    // Public for testing: HMAC verification
    static bool VerifyHmac(
        const std::string& secret,
        const std::string& payload,
        const std::string& signature);

private:
    // Load webhook_config.json
    bool LoadConfig();

    // libsoup request handler callback
    static void HandleRequest(
        SoupServer* server,
        SoupMessage* msg,
        const char* path,
        GHashTable* query,
        SoupClientContext* client,
        gpointer user_data);

    // Process a webhook request (runs on thread)
    void ProcessWebhook(
        SoupMessage* msg,
        const std::string& path,
        const std::string& body);

    AgentCore* agent_;
    SoupServer* server_ = nullptr;
    std::thread server_thread_;
    GMainLoop* loop_ = nullptr;
    std::atomic<bool> running_{false};

    // Configuration
    int port_ = 8080;
    std::string hmac_secret_;
    std::vector<WebhookRoute> routes_;
};

} // namespace tizenclaw

#endif // __WEBHOOK_CHANNEL_H__
