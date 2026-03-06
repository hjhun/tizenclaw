#include "webhook_channel.hh"
#include "agent_core.hh"
#include "../common/logging.hh"

#include <fstream>
#include <cstring>
#include <iomanip>
#include <sstream>

namespace tizenclaw {

WebhookChannel::WebhookChannel(AgentCore* agent)
    : agent_(agent) {
}

WebhookChannel::~WebhookChannel() {
    Stop();
}

bool WebhookChannel::LoadConfig() {
    std::string config_path =
        "/opt/usr/share/tizenclaw/config/"
        "webhook_config.json";
    std::ifstream f(config_path);
    if (!f.is_open()) {
        LOG(WARNING) << "No webhook_config.json "
                     << "found — webhook disabled";
        return false;
    }

    try {
        nlohmann::json j;
        f >> j;
        port_ = j.value("port", 8080);
        hmac_secret_ = j.value("hmac_secret", "");

        if (j.contains("routes") &&
            j["routes"].is_array()) {
            for (auto& r : j["routes"]) {
                WebhookRoute route;
                route.path =
                    r.value("path", "");
                route.session_id =
                    r.value("session_id",
                            "webhook_default");
                if (!route.path.empty()) {
                    routes_.push_back(
                        std::move(route));
                }
            }
        }
    } catch (const std::exception& e) {
        LOG(ERROR) << "Failed to parse "
                   << "webhook_config.json: "
                   << e.what();
        return false;
    }

    if (routes_.empty()) {
        LOG(WARNING) << "No webhook routes "
                     << "configured";
        return false;
    }

    LOG(INFO) << "Webhook config loaded: port="
              << port_ << " routes="
              << routes_.size();
    return true;
}

// HMAC-SHA256 verification using GLib GHmac
bool WebhookChannel::VerifyHmac(
    const std::string& secret,
    const std::string& payload,
    const std::string& signature) {
    if (secret.empty()) {
        // No secret configured — skip verify
        return true;
    }
    if (signature.empty()) {
        return false;
    }

    // Expected format: "sha256=<hex>"
    std::string prefix = "sha256=";
    std::string hex_sig = signature;
    if (hex_sig.substr(0, prefix.size()) ==
        prefix) {
        hex_sig = hex_sig.substr(prefix.size());
    }

    GHmac* hmac = g_hmac_new(
        G_CHECKSUM_SHA256,
        reinterpret_cast<const guchar*>(
            secret.data()),
        secret.size());
    g_hmac_update(
        hmac,
        reinterpret_cast<const guchar*>(
            payload.data()),
        payload.size());

    const gchar* computed =
        g_hmac_get_string(hmac);
    bool match =
        (hex_sig == std::string(computed));
    g_hmac_unref(hmac);

    return match;
}

void WebhookChannel::HandleRequest(
    SoupServer* /*server*/,
    SoupMessage* msg,
    const char* path,
    GHashTable* /*query*/,
    SoupClientContext* /*client*/,
    gpointer user_data) {
    auto* self =
        static_cast<WebhookChannel*>(user_data);

    // Only accept POST requests
    if (msg->method != SOUP_METHOD_POST) {
        soup_message_set_status(
            msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
        soup_message_set_response(
            msg, "application/json",
            SOUP_MEMORY_COPY,
            "{\"error\":\"Method not allowed\"}",
            30);
        return;
    }

    // Extract body
    SoupMessageBody* body = msg->request_body;
    std::string payload;
    if (body && body->data && body->length > 0) {
        payload.assign(body->data, body->length);
    }

    LOG(INFO) << "Webhook request: " << path
              << " (" << payload.size()
              << " bytes)";

    // HMAC signature verification
    if (!self->hmac_secret_.empty()) {
        SoupMessageHeaders* headers =
            msg->request_headers;
        const char* sig_header =
            soup_message_headers_get_one(
                headers,
                "X-Hub-Signature-256");
        std::string sig =
            sig_header ? sig_header : "";

        if (!VerifyHmac(self->hmac_secret_,
                        payload, sig)) {
            LOG(WARNING) << "Webhook HMAC "
                         << "verification failed "
                         << "for " << path;
            soup_message_set_status(
                msg, SOUP_STATUS_FORBIDDEN);
            soup_message_set_response(
                msg, "application/json",
                SOUP_MEMORY_COPY,
                "{\"error\":\"Invalid "
                "signature\"}",
                27);
            return;
        }
    }

    // Find matching route
    std::string req_path(path);
    std::string session_id = "webhook_default";
    bool found = false;
    for (auto& route : self->routes_) {
        if (req_path == route.path) {
            session_id = route.session_id;
            found = true;
            break;
        }
    }

    if (!found) {
        soup_message_set_status(
            msg, SOUP_STATUS_NOT_FOUND);
        soup_message_set_response(
            msg, "application/json",
            SOUP_MEMORY_COPY,
            "{\"error\":\"No matching route\"}",
            30);
        return;
    }

    // Extract text from payload
    std::string prompt;
    try {
        auto j = nlohmann::json::parse(payload);
        prompt = j.value("text", "");
        if (prompt.empty()) {
            // Use the entire payload as prompt
            prompt = payload;
        }
    } catch (...) {
        // Non-JSON payload — use as-is
        prompt = payload;
    }

    if (prompt.empty()) {
        soup_message_set_status(
            msg, SOUP_STATUS_BAD_REQUEST);
        soup_message_set_response(
            msg, "application/json",
            SOUP_MEMORY_COPY,
            "{\"error\":\"Empty payload\"}",
            25);
        return;
    }

    // Process prompt synchronously
    // (libsoup handles threading internally)
    std::string result;
    if (self->agent_) {
        result = self->agent_->ProcessPrompt(
            session_id, prompt);
    } else {
        result = "Error: agent not available";
    }

    // Build response
    nlohmann::json resp = {
        {"status", "ok"},
        {"session_id", session_id},
        {"response", result}
    };
    std::string resp_str = resp.dump();

    soup_message_set_status(msg, SOUP_STATUS_OK);
    soup_message_set_response(
        msg, "application/json",
        SOUP_MEMORY_COPY,
        resp_str.c_str(),
        static_cast<gsize>(resp_str.size()));
}

bool WebhookChannel::Start() {
    if (running_) {
        return true;
    }

    if (!LoadConfig()) {
        return false;
    }

    GError* error = nullptr;
    server_ = soup_server_new(
        SOUP_SERVER_SERVER_HEADER,
        "TizenClaw-Webhook",
        nullptr);

    if (!server_) {
        LOG(ERROR) << "Failed to create "
                   << "SoupServer";
        return false;
    }

    // Register handler for all paths
    soup_server_add_handler(
        server_, "/",
        HandleRequest, this, nullptr);

    // Listen on configured port
    if (!soup_server_listen_all(
            server_, port_,
            static_cast<SoupServerListenOptions>(0),
            &error)) {
        LOG(ERROR) << "Failed to listen on "
                   << "port " << port_
                   << ": " << error->message;
        g_error_free(error);
        g_object_unref(server_);
        server_ = nullptr;
        return false;
    }

    running_ = true;

    // Run GMainLoop in a separate thread
    server_thread_ = std::thread([this]() {
        loop_ = g_main_loop_new(nullptr, FALSE);
        LOG(INFO) << "Webhook server running on "
                  << "port " << port_;
        g_main_loop_run(loop_);
        g_main_loop_unref(loop_);
        loop_ = nullptr;
    });

    LOG(INFO) << "WebhookChannel started on "
              << "port " << port_;
    return true;
}

void WebhookChannel::Stop() {
    if (!running_) {
        return;
    }

    running_ = false;

    if (loop_) {
        g_main_loop_quit(loop_);
    }

    if (server_thread_.joinable()) {
        server_thread_.join();
    }

    if (server_) {
        soup_server_disconnect(server_);
        g_object_unref(server_);
        server_ = nullptr;
    }

    LOG(INFO) << "WebhookChannel stopped.";
}

} // namespace tizenclaw
