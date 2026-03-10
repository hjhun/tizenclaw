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
/**
 * tizenclaw-cli: CLI tool for testing TizenClaw daemon
 * utilizing the libtizenclaw CAPI.
 *
 * Usage:
 *   tizenclaw-cli "What is the battery level?"
 *   tizenclaw-cli -s my_session "Run a skill"
 *   tizenclaw-cli --stream "Tell me about Tizen"
 *   tizenclaw-cli   (interactive mode)
 */

#include "tizenclaw.h"

#include <iostream>
#include <string>
#include <vector>
#include <future>
#include <mutex>

namespace {

struct RequestContext {
  std::promise<std::string> promise;
  std::string response;
};

void OnResponseReady(const char* session_id, const char* response, void* user_data) {
  (void)session_id; // Unused in single-shot CLI
  auto* ctx = static_cast<RequestContext*>(user_data);
  if (ctx) {
    ctx->promise.set_value(response ? response : "");
  }
}

void OnStreamChunk(const char* session_id, const char* chunk, bool is_done, void* user_data) {
  (void)session_id; // Unused
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

void OnErrorCallback(const char* session_id, int error_code, const char* error_message, void* user_data) {
  (void)session_id; // Unused
  auto* ctx = static_cast<RequestContext*>(user_data);
  if (!ctx) return;

  std::cerr << "\n[Error " << error_code << "] " << (error_message ? error_message : "Unknown error") << "\n";
  ctx->promise.set_value("");
}

std::string SendRequestThroughCAPI(tizenclaw_client_h client, const std::string& session_id, const std::string& prompt, bool stream) {
  RequestContext ctx;
  auto future = ctx.promise.get_future();

  int ret;
  if (stream) {
    ret = tizenclaw_client_send_request_stream(client, session_id.c_str(), prompt.c_str(), OnStreamChunk, OnErrorCallback, &ctx);
  } else {
    ret = tizenclaw_client_send_request(client, session_id.c_str(), prompt.c_str(), OnResponseReady, OnErrorCallback, &ctx);
  }

  if (ret != TIZENCLAW_ERROR_NONE) {
    std::cerr << "Failed to send request. Error code: " << ret << "\n";
    return "";
  }

  // Block until complete
  return future.get();
}

void PrintUsage() {
  std::cerr << "tizenclaw-cli — TizenClaw IPC test\n\n"
            << "Usage:\n"
            << "  tizenclaw-cli [options] [prompt]\n\n"
            << "Options:\n"
            << "  -s <id>       Session ID (default: cli_test)\n"
            << "  --stream      Enable streaming\n"
            << "  -h, --help    Show this help\n\n"
            << "If no prompt given, interactive mode.\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  std::string session_id = "cli_test";
  bool stream = false;
  std::string prompt;

  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg == "-h" || arg == "--help") {
      PrintUsage();
      return 0;
    } else if (arg == "-s" && i + 1 < argc) {
      session_id = argv[++i];
    } else if (arg == "--stream") {
      stream = true;
    } else {
      for (int j = i; j < argc; ++j) {
        if (!prompt.empty()) prompt += " ";
        prompt += argv[j];
      }
      break;
    }
  }

  tizenclaw_client_h client = nullptr;
  if (tizenclaw_client_create(&client) != TIZENCLAW_ERROR_NONE) {
    std::cerr << "Failed to create TizenClaw client.\n";
    return 1;
  }

  // Single-shot mode
  if (!prompt.empty()) {
    std::string resp = SendRequestThroughCAPI(client, session_id, prompt, stream);
    if (!stream && !resp.empty()) {
      std::cout << resp << "\n";
    }
    tizenclaw_client_destroy(client);
    return resp.empty() ? 1 : 0;
  }

  // Interactive mode
  std::cout << "TizenClaw CLI (session: " << session_id << ")\n"
            << "Type a prompt and press Enter. Ctrl+D to exit.\n\n";

  while (true) {
    std::cout << "you> " << std::flush;
    std::string line;
    if (!std::getline(std::cin, line)) break;
    if (line.empty()) continue;

    std::string resp = SendRequestThroughCAPI(client, session_id, line, stream);
    if (!stream && !resp.empty()) {
      std::cout << "\nassistant> " << resp << "\n\n";
    }
  }

  tizenclaw_client_destroy(client);
  std::cout << "\nBye.\n";
  return 0;
}
