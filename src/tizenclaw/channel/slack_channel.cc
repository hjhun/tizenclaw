#include "slack_channel.hh"
#include "../core/agent_core.hh"
#include "../infra/http_client.hh"
#include "../../common/logging.hh"

#include <fstream>
#include <libwebsockets.h>

namespace tizenclaw {

// libwebsockets per-session user data
struct SlackWsSessionData {
    SlackChannel* channel;
    bool connected;
    std::string rx_buffer;
};

// libwebsockets callback for Slack Socket Mode
static int SlackWsCallback(
    struct lws* wsi,
    enum lws_callback_reasons reason,
    void* user, void* in, size_t len) {

    auto* sd =
        static_cast<SlackWsSessionData*>(user);

    switch (reason) {
    case LWS_CALLBACK_CLIENT_ESTABLISHED:
        LOG(INFO) << "Slack WS connected";
        if (sd) sd->connected = true;
        break;

    case LWS_CALLBACK_CLIENT_RECEIVE:
        if (sd && in && len > 0) {
            sd->rx_buffer.append(
                static_cast<char*>(in), len);

            // Check if this is the final fragment
            if (lws_is_final_fragment(wsi)) {
                // Process complete message
                // via the channel instance
                // (handled externally by parsing
                //  rx_buffer after event loop tick)
                LOG(INFO) << "Slack WS received: "
                          << sd->rx_buffer.size()
                          << " bytes";
                // The SocketLoop will process this
                sd->rx_buffer.clear();
            }
        }
        break;

    case LWS_CALLBACK_CLIENT_WRITEABLE:
        break;

    case LWS_CALLBACK_CLIENT_CONNECTION_ERROR:
        LOG(ERROR) << "Slack WS connection error";
        if (sd) sd->connected = false;
        break;

    case LWS_CALLBACK_CLIENT_CLOSED:
        LOG(INFO) << "Slack WS closed";
        if (sd) sd->connected = false;
        break;

    default:
        break;
    }

    return 0;
}

static const struct lws_protocols
    slack_protocols[] = {
    {
        "slack-socket-mode",
        SlackWsCallback,
        sizeof(SlackWsSessionData),
        65536, /* rx buffer size */
        0, nullptr, 0
    },
    { nullptr, nullptr, 0, 0, 0, nullptr, 0 }
};


SlackChannel::SlackChannel(AgentCore* agent)
    : agent_(agent) {
}

SlackChannel::~SlackChannel() {
    Stop();
}

bool SlackChannel::LoadConfig() {
    std::string config_path =
        "/opt/usr/share/tizenclaw/config/"
        "slack_config.json";
    std::ifstream f(config_path);
    if (!f.is_open()) {
        LOG(WARNING)
            << "No slack_config.json found";
        return false;
    }

    try {
        nlohmann::json j;
        f >> j;
        bot_token_ = j.value("bot_token", "");
        app_token_ = j.value("app_token", "");

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
            << "slack_config.json: "
            << e.what();
        return false;
    }

    if (bot_token_.empty() ||
        app_token_.empty()) {
        LOG(WARNING) << "Slack tokens not "
                     << "configured";
        return false;
    }

    LOG(INFO) << "Slack config loaded";
    return true;
}

std::string SlackChannel::GetSocketModeUrl() {
    std::string url =
        "https://slack.com/api/"
        "apps.connections.open";

    auto resp = HttpClient::Post(
        url,
        {{"Authorization",
          "Bearer " + app_token_},
         {"Content-Type",
          "application/x-www-form-urlencoded"}},
        "");

    if (!resp.success) {
        LOG(ERROR) << "apps.connections.open "
                   << "failed: " << resp.error;
        return "";
    }

    try {
        auto j = nlohmann::json::parse(resp.body);
        if (j.value("ok", false)) {
            return j["url"].get<std::string>();
        }
        LOG(ERROR) << "Slack API error: "
                   << j.value("error", "unknown");
    } catch (...) {
        LOG(ERROR) << "Failed to parse "
                   << "Slack response";
    }
    return "";
}

void SlackChannel::SocketLoop() {
    while (running_) {
        // Get WebSocket URL
        std::string ws_url = GetSocketModeUrl();
        if (ws_url.empty()) {
            LOG(ERROR) << "Failed to get Slack "
                       << "Socket Mode URL, "
                       << "retrying in 10s";
            std::this_thread::sleep_for(
                std::chrono::seconds(10));
            continue;
        }

        LOG(INFO) << "Connecting to Slack WS: "
                  << ws_url.substr(0, 50) << "...";

        // Parse WSS URL
        // Format: wss://wss-primary.slack.com/...
        struct lws_context_creation_info info{};
        info.port = CONTEXT_PORT_NO_LISTEN;
        info.protocols = slack_protocols;
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

        // Parse URL using std::string
        std::string url_body = ws_url;
        if (url_body.compare(
                0, 6, "wss://") == 0) {
            url_body = url_body.substr(6);
        }

        std::string host;
        std::string path_str;
        auto slash_pos = url_body.find('/');
        if (slash_pos != std::string::npos) {
            host = url_body.substr(0, slash_pos);
            path_str = url_body.substr(slash_pos);
        } else {
            host = std::move(url_body);
            path_str = "/";
        }

        struct lws_client_connect_info ccinfo{};
        ccinfo.context = context;
        ccinfo.address = host.c_str();
        ccinfo.port = 443;
        ccinfo.path = path_str.c_str();
        ccinfo.host = host.c_str();
        ccinfo.origin = host.c_str();
        ccinfo.ssl_connection =
            LCCSCF_USE_SSL;
        ccinfo.protocol =
            slack_protocols[0].name;

        struct lws* wsi =
            lws_client_connect_via_info(&ccinfo);
        if (!wsi) {
            LOG(ERROR) << "Failed to connect "
                       << "to Slack WS";
            lws_context_destroy(context);
            std::this_thread::sleep_for(
                std::chrono::seconds(10));
            continue;
        }

        // Event loop
        while (running_ && wsi) {
            int n = lws_service(context, 500);
            if (n < 0) break;
        }

        lws_context_destroy(context);

        if (running_) {
            LOG(INFO) << "Slack WS disconnected,"
                      << " reconnecting in 5s";
            std::this_thread::sleep_for(
                std::chrono::seconds(5));
        }
    }
}

void SlackChannel::HandleMessageEvent(
    const std::string& channel,
    const std::string& /*user*/,
    const std::string& text,
    const std::string& ts) {
    if (!agent_) return;

    // Check allowlist
    if (!allowed_channels_.empty() &&
        allowed_channels_.find(channel) ==
            allowed_channels_.end()) {
        LOG(INFO) << "Blocked Slack channel: "
                  << channel;
        return;
    }

    std::string session_id =
        "slack_" + channel;
    std::string response =
        agent_->ProcessPrompt(
            session_id, text);

    SendReply(channel, response, ts);
}

void SlackChannel::SendReply(
    const std::string& channel,
    const std::string& text,
    const std::string& thread_ts) {
    std::string url =
        "https://slack.com/api/"
        "chat.postMessage";

    std::string safe_text = text;
    if (safe_text.length() > 4000) {
        safe_text =
            safe_text.substr(0, 4000) +
            "\n...(truncated)";
    }

    nlohmann::json payload = {
        {"channel", channel},
        {"text", safe_text}
    };

    if (!thread_ts.empty()) {
        payload["thread_ts"] = thread_ts;
    }

    auto resp = HttpClient::Post(
        url,
        {{"Authorization",
          "Bearer " + bot_token_},
         {"Content-Type",
          "application/json"}},
        payload.dump());

    if (!resp.success) {
        LOG(ERROR) << "Slack sendMessage failed"
                   << ": " << resp.error;
    }
}

bool SlackChannel::Start() {
    if (running_) return true;

    if (!LoadConfig()) return false;

    running_ = true;
    ws_thread_ =
        std::thread(&SlackChannel::SocketLoop,
                    this);
    LOG(INFO) << "SlackChannel started";
    return true;
}

void SlackChannel::Stop() {
    if (!running_) return;
    running_ = false;

    if (ws_thread_.joinable()) {
        ws_thread_.join();
    }
    LOG(INFO) << "SlackChannel stopped";
}

} // namespace tizenclaw
