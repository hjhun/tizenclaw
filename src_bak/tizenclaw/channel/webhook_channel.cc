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
#include "webhook_channel.hh"

#include <condition_variable>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <mutex>
#include <sstream>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"

namespace tizenclaw {

WebhookChannel::WebhookChannel(AgentCore* agent) : agent_(agent) {}

WebhookChannel::~WebhookChannel() { Stop(); }

bool WebhookChannel::LoadConfig() {
  std::string config_path =
      "/opt/usr/share/tizenclaw/config/"
      "webhook_config.json";
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING) << "No webhook_config.json " << "found — webhook disabled";
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;
    port_ = j.value("port", 8080);
    hmac_secret_ = j.value("hmac_secret", "");

    if (j.contains("routes") && j["routes"].is_array()) {
      for (auto& r : j["routes"]) {
        WebhookRoute route;
        route.path = r.value("path", "");
        route.session_id = r.value("session_id", "webhook_default");
        if (!route.path.empty()) {
          routes_.push_back(std::move(route));
        }
      }
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse " << "webhook_config.json: " << e.what();
    return false;
  }

  if (routes_.empty()) {
    LOG(WARNING) << "No webhook routes " << "configured";
    return false;
  }

  LOG(INFO) << "Webhook config loaded: port=" << port_
            << " routes=" << routes_.size();
  return true;
}

// HMAC-SHA256 verification using GLib GHmac
bool WebhookChannel::VerifyHmac(const std::string& secret,
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
  if (hex_sig.substr(0, prefix.size()) == prefix) {
    hex_sig = hex_sig.substr(prefix.size());
  }

  GHmac* hmac =
      g_hmac_new(G_CHECKSUM_SHA256,
                 reinterpret_cast<const guchar*>(secret.data()), secret.size());
  g_hmac_update(hmac, reinterpret_cast<const guchar*>(payload.data()),
                payload.size());

  const gchar* computed = g_hmac_get_string(hmac);
  bool match = (hex_sig == std::string(computed));
  g_hmac_unref(hmac);

  return match;
}

void WebhookChannel::HandleRequest(SoupServer* server, SoupMessage* msg,
                                   const char* path, GHashTable* /*query*/,
                                   SoupClientContext* /*client*/,
                                   gpointer user_data) {
  // CRITICAL: C callback from libsoup. Any unhandled C++ exception
  // causes std::terminate() → SIGABRT. Wrap entire body.
  try {
    auto* self = static_cast<WebhookChannel*>(user_data);

    // Only accept POST requests
    if (msg->method != SOUP_METHOD_POST) {
      soup_message_set_status(msg, SOUP_STATUS_METHOD_NOT_ALLOWED);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                "{\"error\":\"Method not allowed\"}", 30);
      return;
    }

    // Extract body
    SoupMessageBody* body = msg->request_body;
    std::string payload;
    if (body && body->data && body->length > 0) {
      payload.assign(body->data, body->length);
    }

    LOG(INFO) << "Webhook request: " << path << " (" << payload.size()
              << " bytes)";

    // HMAC signature verification
    if (!self->hmac_secret_.empty()) {
      SoupMessageHeaders* headers = msg->request_headers;
      const char* sig_header =
          soup_message_headers_get_one(headers, "X-Hub-Signature-256");
      std::string sig = sig_header ? sig_header : "";

      if (!VerifyHmac(self->hmac_secret_, payload, sig)) {
        LOG(WARNING) << "Webhook HMAC " << "verification failed " << "for "
                     << path;
        soup_message_set_status(msg, SOUP_STATUS_FORBIDDEN);
        soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
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
      soup_message_set_status(msg, SOUP_STATUS_NOT_FOUND);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                "{\"error\":\"No matching route\"}", 30);
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
      soup_message_set_status(msg, SOUP_STATUS_BAD_REQUEST);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                "{\"error\":\"Empty payload\"}", 25);
      return;
    }

    // Reject new requests during shutdown to prevent Stop() deadlock.
    if (!self->running_.load()) {
      soup_message_set_status(msg, SOUP_STATUS_SERVICE_UNAVAILABLE);
      soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                                "{\"error\":\"Shutting down\"}", 24);
      return;
    }

    // Run ProcessPrompt on a worker thread to avoid
    // blocking the GMainLoop (prevents deadlock).
    g_object_ref(msg);  // prevent libsoup from freeing msg early
    soup_server_pause_message(server, msg);

    struct WebhookCtx {
      SoupServer* server;
      SoupMessage* msg;
      AgentCore* agent;
      GMainContext* context;
      std::string session_id;
      std::string result;
      WebhookChannel* channel;
    };

    auto* ctx = new WebhookCtx{
        server, msg, self->agent_, self->context_, session_id, "", self};

    self->pending_workers_.fetch_add(1);
    std::thread([ctx, prompt]() {
      if (ctx->agent) {
        try {
          ctx->result = ctx->agent->ProcessPrompt(
              ctx->session_id, prompt);
        } catch (const std::exception& e) {
          ctx->result = std::string("Error: ") + e.what();
        } catch (...) {
          ctx->result = "Unknown internal error";
        }
      } else {
        ctx->result = "Error: agent not available";
      }

      g_main_context_invoke(
          ctx->context,
          [](gpointer data) -> gboolean {
            auto* c = static_cast<WebhookCtx*>(data);
            try {
              nlohmann::json resp = {
                  {"status", "ok"},
                  {"session_id", c->session_id},
                  {"response", c->result}};
              std::string resp_str = resp.dump(-1, ' ', false, nlohmann::json::error_handler_t::replace);

              soup_message_set_status(
                  c->msg, SOUP_STATUS_OK);
              soup_message_set_response(
                  c->msg, "application/json",
                  SOUP_MEMORY_COPY,
                  resp_str.c_str(),
                  static_cast<gsize>(resp_str.size()));
            } catch (...) {
              soup_message_set_status(c->msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
            }
            soup_server_unpause_message(
                c->server, c->msg);
            g_object_unref(c->msg);  // release our ref

            // Decrement pending workers and notify Stop() on the
            // GMainLoop thread to avoid use-after-free on the
            // detached worker thread.
            auto* ch = c->channel;
            delete c;
            ch->pending_workers_.fetch_sub(1);
            {
              std::lock_guard<std::mutex> lk(ch->workers_mutex_);
              ch->workers_cv_.notify_all();
            }
            return G_SOURCE_REMOVE;
          },
          ctx);
    }).detach();
  } catch (const std::exception& e) {
    LOG(ERROR) << "Webhook HandleRequest exception: " << e.what();
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Internal error\"}", 25);
  } catch (...) {
    LOG(ERROR) << "Webhook HandleRequest unknown exception";
    soup_message_set_status(msg, SOUP_STATUS_INTERNAL_SERVER_ERROR);
    soup_message_set_response(msg, "application/json", SOUP_MEMORY_COPY,
                              "{\"error\":\"Internal error\"}", 25);
  }
}

bool WebhookChannel::Start() {
  if (running_) {
    return true;
  }

  if (!LoadConfig()) {
    return false;
  }

  // Create a dedicated GMainContext so that SoupServer and
  // g_main_context_invoke() callbacks all run on the server thread.
  context_ = g_main_context_new();

  std::mutex mtx;
  std::condition_variable cv;
  bool ready = false;
  bool ok = false;

  server_thread_ = std::thread([this, &mtx, &cv, &ready, &ok]() {
    g_main_context_push_thread_default(context_);

    server_ = soup_server_new(
        SOUP_SERVER_SERVER_HEADER, "TizenClaw-Webhook", nullptr);
    if (!server_) {
      LOG(ERROR) << "Failed to create " << "SoupServer";
      std::lock_guard<std::mutex> lk(mtx);
      ok = false;
      ready = true;
      cv.notify_one();
      g_main_context_pop_thread_default(context_);
      return;
    }

    soup_server_add_handler(server_, "/", HandleRequest, this, nullptr);

    GError* error = nullptr;
    if (!soup_server_listen_all(
            server_, port_,
            static_cast<SoupServerListenOptions>(0), &error)) {
      LOG(ERROR) << "Failed to listen on "
                 << "port " << port_ << ": "
                 << error->message;
      g_error_free(error);
      g_object_unref(server_);
      server_ = nullptr;
      std::lock_guard<std::mutex> lk(mtx);
      ok = false;
      ready = true;
      cv.notify_one();
      g_main_context_pop_thread_default(context_);
      return;
    }

    loop_ = g_main_loop_new(context_, FALSE);

    {
      std::lock_guard<std::mutex> lk(mtx);
      ok = true;
      ready = true;
      cv.notify_one();
    }

    LOG(INFO) << "Webhook server running on " << "port " << port_;
    g_main_loop_run(loop_);
    g_main_loop_unref(loop_);
    loop_ = nullptr;
    g_main_context_pop_thread_default(context_);
  });

  {
    std::unique_lock<std::mutex> lk(mtx);
    cv.wait(lk, [&ready]() { return ready; });
  }

  if (!ok) {
    if (server_thread_.joinable()) server_thread_.join();
    g_main_context_unref(context_);
    context_ = nullptr;
    return false;
  }

  running_ = true;
  LOG(INFO) << "WebhookChannel started on " << "port " << port_;
  return true;
}

void WebhookChannel::Stop() {
  if (!running_) {
    return;
  }

  running_ = false;

  // Wait for all pending worker threads to finish
  // before destroying server/context resources.
  {
    std::unique_lock<std::mutex> lk(workers_mutex_);
    workers_cv_.wait(lk, [this]() {
      return pending_workers_.load() == 0;
    });
  }

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

  if (context_) {
    g_main_context_unref(context_);
    context_ = nullptr;
  }

  LOG(INFO) << "WebhookChannel stopped.";
}

}  // namespace tizenclaw
