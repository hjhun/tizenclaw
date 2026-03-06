#include "discord_channel.hh"
#include "agent_core.hh"
#include "http_client.hh"
#include "../common/logging.hh"

#include <fstream>
#include <libwebsockets.h>
#include <cstring>

namespace tizenclaw {

// Gateway intents for message content
static constexpr int kGuildMessages = (1 << 9);
static constexpr int kMessageContent = (1 << 15);

// libwebsockets per-session user data
struct DiscordWsSessionData {
    DiscordChannel* channel;
    bool connected;
    std::string rx_buffer;
    int heartbeat_interval;
    int sequence;
};

// libwebsockets callback for Discord Gateway
static int DiscordWsCallback(
    struct lws* wsi,
    enum lws_callback_reasons reason,
    void* user, void* in, size_t len) {

    auto* sd =
        static_cast<DiscordWsSessionData*>(user);

    switch (reason) {
    case LWS_CALLBACK_CLIENT_ESTABLISHED:
        LOG(INFO) << "Discord WS connected";
        if (sd) sd->connected = true;
        break;

    case LWS_CALLBACK_CLIENT_RECEIVE:
        if (sd && in && len > 0) {
            sd->rx_buffer.append(
                static_cast<char*>(in), len);

            if (lws_is_final_fragment(wsi)) {
                LOG(INFO) << "Discord WS "
                          << "received: "
                          << sd->rx_buffer.size()
                          << " bytes";
                sd->rx_buffer.clear();
            }
        }
        break;

    case LWS_CALLBACK_CLIENT_WRITEABLE:
        break;

    case LWS_CALLBACK_CLIENT_CONNECTION_ERROR:
        LOG(ERROR) << "Discord WS "
                   << "connection error";
        if (sd) sd->connected = false;
        break;

    case LWS_CALLBACK_CLIENT_CLOSED:
        LOG(INFO) << "Discord WS closed";
        if (sd) sd->connected = false;
        break;

    default:
        break;
    }

    return 0;
}

static const struct lws_protocols
    discord_protocols[] = {
    {
        "discord-gateway",
        DiscordWsCallback,
        sizeof(DiscordWsSessionData),
        65536
    },
    { nullptr, nullptr, 0, 0 }
};


DiscordChannel::DiscordChannel(AgentCore* agent)
    : agent_(agent),
      intents_(kGuildMessages | kMessageContent) {
}

DiscordChannel::~DiscordChannel() {
    Stop();
}

bool DiscordChannel::LoadConfig() {
    std::string config_path =
        "/opt/usr/share/tizenclaw/config/"
        "discord_config.json";
    std::ifstream f(config_path);
    if (!f.is_open()) {
        LOG(WARNING)
            << "No discord_config.json found";
        return false;
    }

    try {
        nlohmann::json j;
        f >> j;
        bot_token_ = j.value("bot_token", "");

        if (j.contains("allowed_guilds") &&
            j["allowed_guilds"].is_array()) {
            for (auto& g :
                 j["allowed_guilds"]) {
                allowed_guilds_.insert(
                    g.get<std::string>());
            }
        }

        if (j.contains("allowed_channels") &&
            j["allowed_channels"].is_array()) {
            for (auto& ch :
                 j["allowed_channels"]) {
                allowed_channels_.insert(
                    ch.get<std::string>());
            }
        }
    } catch (const std::exception& e) {
        LOG(ERROR)
            << "Failed to parse "
            << "discord_config.json: "
            << e.what();
        return false;
    }

    if (bot_token_.empty()) {
        LOG(WARNING) << "Discord bot_token "
                     << "not configured";
        return false;
    }

    LOG(INFO) << "Discord config loaded";
    return true;
}

std::string DiscordChannel::GetGatewayUrl() {
    std::string url =
        "https://discord.com/api/v10/"
        "gateway/bot";

    auto resp = HttpClient::Get(
        url,
        {{"Authorization",
          "Bot " + bot_token_}});

    if (!resp.success) {
        LOG(ERROR) << "gateway/bot failed: "
                   << resp.error;
        return "";
    }

    try {
        auto j = nlohmann::json::parse(resp.body);
        std::string ws_url =
            j.value("url", "");
        if (!ws_url.empty()) {
            ws_url += "/?v=10&encoding=json";
        }
        return ws_url;
    } catch (...) {
        LOG(ERROR) << "Failed to parse "
                   << "gateway response";
    }
    return "";
}

void DiscordChannel::GatewayLoop() {
    while (running_) {
        std::string ws_url = GetGatewayUrl();
        if (ws_url.empty()) {
            LOG(ERROR) << "Failed to get Discord"
                       << " Gateway URL, "
                       << "retrying in 10s";
            std::this_thread::sleep_for(
                std::chrono::seconds(10));
            continue;
        }

        LOG(INFO) << "Connecting to Discord GW: "
                  << ws_url.substr(0, 50) << "...";

        struct lws_context_creation_info info;
        memset(&info, 0, sizeof(info));
        info.port = CONTEXT_PORT_NO_LISTEN;
        info.protocols = discord_protocols;
        info.options =
            LWS_SERVER_OPTION_DO_SSL_GLOBAL_INIT;

        struct lws_context* context =
            lws_create_context(&info);
        if (!context) {
            LOG(ERROR) << "Failed to create "
                       << "lws context";
            std::this_thread::sleep_for(
                std::chrono::seconds(10));
            continue;
        }

        // Parse URL
        char host[256] = {0};
        char path[2048] = {0};
        const char* url_str = ws_url.c_str();
        const char* host_start = url_str;
        if (strncmp(url_str, "wss://", 6) == 0) {
            host_start = url_str + 6;
        }

        const char* path_start =
            strchr(host_start, '/');
        if (path_start) {
            size_t host_len =
                path_start - host_start;
            strncpy(host, host_start,
                std::min(host_len, (size_t)255));
            strncpy(path, path_start,
                std::min(strlen(path_start),
                         (size_t)2047));
        } else {
            strncpy(host, host_start, 255);
            strcpy(path, "/");
        }

        struct lws_client_connect_info ccinfo;
        memset(&ccinfo, 0, sizeof(ccinfo));
        ccinfo.context = context;
        ccinfo.address = host;
        ccinfo.port = 443;
        ccinfo.path = path;
        ccinfo.host = host;
        ccinfo.origin = host;
        ccinfo.ssl_connection =
            LCCSCF_USE_SSL;
        ccinfo.protocol =
            discord_protocols[0].name;

        struct lws* wsi =
            lws_client_connect_via_info(&ccinfo);
        if (!wsi) {
            LOG(ERROR) << "Failed to connect "
                       << "to Discord GW";
            lws_context_destroy(context);
            std::this_thread::sleep_for(
                std::chrono::seconds(10));
            continue;
        }

        while (running_ && wsi) {
            int n = lws_service(context, 500);
            if (n < 0) break;
        }

        lws_context_destroy(context);

        if (running_) {
            LOG(INFO) << "Discord GW "
                      << "disconnected, "
                      << "reconnecting in 5s";
            std::this_thread::sleep_for(
                std::chrono::seconds(5));
        }
    }
}

void DiscordChannel::HandleMessageCreate(
    const nlohmann::json& data) {
    if (!agent_) return;

    // Ignore bot messages
    if (data.contains("author") &&
        data["author"].value("bot", false)) {
        return;
    }

    std::string channel_id =
        data.value("channel_id", "");
    std::string guild_id =
        data.value("guild_id", "");
    std::string text =
        data.value("content", "");

    if (text.empty()) return;

    // Check guild allowlist
    if (!allowed_guilds_.empty() &&
        allowed_guilds_.find(guild_id) ==
            allowed_guilds_.end()) {
        return;
    }

    // Check channel allowlist
    if (!allowed_channels_.empty() &&
        allowed_channels_.find(channel_id) ==
            allowed_channels_.end()) {
        return;
    }

    std::string session_id =
        "discord_" + channel_id;
    std::string response =
        agent_->ProcessPrompt(
            session_id, text);

    SendReply(channel_id, response);
}

void DiscordChannel::SendReply(
    const std::string& channel_id,
    const std::string& text) {
    std::string url =
        "https://discord.com/api/v10/"
        "channels/" + channel_id + "/messages";

    std::string safe_text = text;
    if (safe_text.length() > 2000) {
        safe_text =
            safe_text.substr(0, 2000 - 20) +
            "\n...(truncated)";
    }

    nlohmann::json payload = {
        {"content", safe_text}
    };

    auto resp = HttpClient::Post(
        url,
        {{"Authorization",
          "Bot " + bot_token_},
         {"Content-Type",
          "application/json"}},
        payload.dump());

    if (!resp.success) {
        LOG(ERROR) << "Discord sendMessage "
                   << "failed: " << resp.error;
    }
}

bool DiscordChannel::Start() {
    if (running_) return true;

    if (!LoadConfig()) return false;

    running_ = true;
    ws_thread_ =
        std::thread(
            &DiscordChannel::GatewayLoop,
            this);
    LOG(INFO) << "DiscordChannel started";
    return true;
}

void DiscordChannel::Stop() {
    if (!running_) return;
    running_ = false;

    if (ws_thread_.joinable()) {
        ws_thread_.join();
    }
    LOG(INFO) << "DiscordChannel stopped";
}

} // namespace tizenclaw
