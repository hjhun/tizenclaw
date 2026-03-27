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

#include "request_handler.hh"

#include <future>
#include <iostream>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

struct RequestContext {
  std::promise<std::string> promise;
  std::string response;
};

void OnResponseReady(const char* session_id,
                     const char* response,
                     void* user_data) {
  (void)session_id;
  auto* ctx = static_cast<RequestContext*>(user_data);
  if (ctx) {
    ctx->promise.set_value(
        response ? response : "");
  }
}

void OnStreamChunk(const char* session_id,
                   const char* chunk,
                   bool is_done,
                   void* user_data) {
  (void)session_id;
  auto* ctx = static_cast<RequestContext*>(user_data);
  if (!ctx) return;

  if (chunk) {
    if (!is_done) {
      std::cout << chunk << std::flush;
    } else {
      ctx->response = chunk;
      std::cout << "\n";
    }
  }

  if (is_done) {
    ctx->promise.set_value(ctx->response);
  }
}

void OnErrorCallback(const char* session_id,
                     int error_code,
                     const char* error_message,
                     void* user_data) {
  (void)session_id;
  auto* ctx = static_cast<RequestContext*>(user_data);
  if (!ctx) return;

  std::cerr << "\n[Error " << error_code << "] "
            << (error_message ? error_message
                              : "Unknown error")
            << "\n";
  ctx->promise.set_value("");
}

}  // namespace

RequestHandler::RequestHandler()
    : client_(nullptr) {}

RequestHandler::~RequestHandler() {
  if (client_) {
    tizenclaw_client_destroy(client_);
    client_ = nullptr;
  }
}

bool RequestHandler::Create() {
  if (tizenclaw_client_create(&client_) !=
      TIZENCLAW_ERROR_NONE) {
    std::cerr
        << "Failed to create TizenClaw client.\n";
    return false;
  }
  return true;
}

std::string RequestHandler::SendRequest(
    const std::string& session_id,
    const std::string& prompt,
    bool stream) {
  RequestContext ctx;
  auto future = ctx.promise.get_future();

  int ret;
  if (stream) {
    ret = tizenclaw_client_send_request_stream(
        client_, session_id.c_str(),
        prompt.c_str(), OnStreamChunk,
        OnErrorCallback, &ctx);
  } else {
    ret = tizenclaw_client_send_request(
        client_, session_id.c_str(),
        prompt.c_str(), OnResponseReady,
        OnErrorCallback, &ctx);
  }

  if (ret != TIZENCLAW_ERROR_NONE) {
    std::cerr << "Failed to send request. "
              << "Error code: " << ret << "\n";
    return "";
  }

  // Block until complete
  return future.get();
}

}  // namespace cli
}  // namespace tizenclaw
