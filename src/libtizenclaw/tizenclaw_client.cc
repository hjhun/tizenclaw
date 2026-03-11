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

#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <atomic>
#include <cstring>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "tizenclaw.h"

#undef EXPORT
#define EXPORT __attribute__((visibility("default")))

#undef API
#define API extern "C" EXPORT

namespace {

/**
 * Internal implementation of the TizenClaw C-API.
 */
class TizenClawClientImpl {
 public:
  TizenClawClientImpl() {}

  ~TizenClawClientImpl() {
    is_destroyed_ = true;
    std::lock_guard<std::mutex> lock(mutex_);
    for (auto& th : worker_threads_) {
      if (th.joinable()) {
        th.join();
      }
    }
  }

  // Non-streaming request
  int SendRequest(const std::string& session_id, const std::string& prompt,
                  tizenclaw_response_cb response_cb,
                  tizenclaw_error_cb error_cb, void* user_data) {
    std::lock_guard<std::mutex> lock(mutex_);
    worker_threads_.emplace_back(&TizenClawClientImpl::PerformRequestTask, this,
                                 session_id, prompt, false, response_cb,
                                 nullptr, error_cb, user_data);
    return TIZENCLAW_ERROR_NONE;
  }

  // Streaming request
  int SendRequestStream(const std::string& session_id,
                        const std::string& prompt,
                        tizenclaw_stream_cb stream_cb,
                        tizenclaw_error_cb error_cb, void* user_data) {
    std::lock_guard<std::mutex> lock(mutex_);
    worker_threads_.emplace_back(&TizenClawClientImpl::PerformRequestTask, this,
                                 session_id, prompt, true, nullptr, stream_cb,
                                 error_cb, user_data);
    return TIZENCLAW_ERROR_NONE;
  }

 private:
  // Helper to escape JSON strings
  static std::string JsonEscape(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 8);
    for (char c : s) {
      switch (c) {
        case '"':
          out += "\\\"";
          break;
        case '\\':
          out += "\\\\";
          break;
        case '\n':
          out += "\\n";
          break;
        case '\r':
          out += "\\r";
          break;
        case '\t':
          out += "\\t";
          break;
        default:
          out += c;
          break;
      }
    }
    return out;
  }

  // Core background threaded runloop for reading sockets to maintain async
  // behavior
  void PerformRequestTask(std::string session_id, std::string prompt,
                          bool stream, tizenclaw_response_cb response_cb,
                          tizenclaw_stream_cb stream_cb,
                          tizenclaw_error_cb error_cb, void* user_data);

  std::vector<std::thread> worker_threads_;
  std::mutex mutex_;
  std::atomic<bool> is_destroyed_{false};
};

bool RecvExact(int fd, void* buf, size_t n) {
  auto p = static_cast<char*>(buf);
  size_t got = 0;
  while (got < n) {
    ssize_t r = ::recv(fd, p + got, n - got, 0);
    if (r <= 0) return false;
    got += static_cast<size_t>(r);
  }
  return true;
}

bool SendAll(int fd, const void* buf, size_t n) {
  auto p = static_cast<const char*>(buf);
  size_t sent = 0;
  while (sent < n) {
    ssize_t w = ::write(fd, p + sent, n - sent);
    if (w <= 0) return false;
    sent += static_cast<size_t>(w);
  }
  return true;
}

int ConnectToSocket() {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    return -1;
  }

  struct sockaddr_un addr = {};
  addr.sun_family = AF_UNIX;
  // Abstract namespace: \0tizenclaw.sock
  const char kName[] = "tizenclaw.sock";
  for (size_t i = 0; i < sizeof(kName) - 1; ++i) {
    addr.sun_path[1 + i] = kName[i];
  }
  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path) + 1 + sizeof(kName) - 1;

  if (connect(sock, reinterpret_cast<struct sockaddr*>(&addr), addr_len) < 0) {
    close(sock);
    return -1;
  }
  return sock;
}

std::string ExtractJsonValue(const std::string& json, const std::string& key) {
  std::string needle = "\"" + key + "\":\"";
  auto pos = json.find(needle);
  if (pos == std::string::npos) return "";
  pos += needle.size();
  std::string val;
  for (size_t i = pos; i < json.size(); ++i) {
    if (json[i] == '"' && (i == 0 || json[i - 1] != '\\')) break;
    val += json[i];
  }
  return val;
}

void TizenClawClientImpl::PerformRequestTask(std::string session_id,
                                             std::string prompt, bool stream,
                                             tizenclaw_response_cb response_cb,
                                             tizenclaw_stream_cb stream_cb,
                                             tizenclaw_error_cb error_cb,
                                             void* user_data) {
  if (is_destroyed_) return;

  int sock = ConnectToSocket();
  if (sock < 0) {
    if (error_cb) {
      error_cb(session_id.c_str(), TIZENCLAW_ERROR_CONNECTION_REFUSED,
               "Failed to connect to UDS daemon tizenclaw.sock", user_data);
    }
    return;
  }

  std::string json_req =
      "{\"jsonrpc\":\"2.0\",\"method\":\"prompt\",\"id\":1,\"params\":{"
      "\"session_id\":\"" +
      JsonEscape(session_id) +
      "\","
      "\"text\":\"" +
      JsonEscape(prompt) +
      "\","
      "\"stream\":" +
      (stream ? "true" : "false") + "}}";

  uint32_t net_len = htonl(static_cast<uint32_t>(json_req.size()));
  if (!SendAll(sock, &net_len, 4) ||
      !SendAll(sock, json_req.data(), json_req.size())) {
    close(sock);
    if (error_cb) {
      error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
               "Failed to send request headers or body", user_data);
    }
    return;
  }

  if (stream) {
    while (!is_destroyed_) {
      uint32_t resp_net_len = 0;
      if (!RecvExact(sock, &resp_net_len, 4)) {
        if (error_cb)
          error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                   "Socket closed or recv header failed", user_data);
        break;
      }
      uint32_t resp_len = ntohl(resp_net_len);
      if (resp_len > 10 * 1024 * 1024) {
        if (error_cb)
          error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                   "Response too large", user_data);
        break;
      }
      std::vector<char> buf(resp_len);
      if (!RecvExact(sock, buf.data(), resp_len)) {
        if (error_cb)
          error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                   "Socket body chunk failed", user_data);
        break;
      }
      std::string chunk_str(buf.data(), resp_len);

      std::string method = ExtractJsonValue(chunk_str, "method");
      std::string text = ExtractJsonValue(chunk_str, "text");

      if (method == "stream_chunk") {
        if (stream_cb)
          stream_cb(session_id.c_str(), text.c_str(), false, user_data);
      } else if (chunk_str.find("\"result\":") != std::string::npos ||
                 chunk_str.find("\"error\":") != std::string::npos) {
        // Complete
        if (stream_cb)
          stream_cb(session_id.c_str(), text.c_str(), true, user_data);
        break;
      } else {
        // Fallback / Unknown
        if (stream_cb)
          stream_cb(session_id.c_str(), chunk_str.c_str(), true, user_data);
        break;
      }
    }
  } else {
    // Single chunk response
    uint32_t resp_net_len = 0;
    if (!RecvExact(sock, &resp_net_len, 4)) {
      close(sock);
      if (error_cb)
        error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                 "Failed to read header", user_data);
      return;
    }
    uint32_t resp_len = ntohl(resp_net_len);
    if (resp_len > 10 * 1024 * 1024) {
      close(sock);
      if (error_cb)
        error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                 "Response too large", user_data);
      return;
    }
    std::vector<char> buf(resp_len);
    if (!RecvExact(sock, buf.data(), resp_len)) {
      close(sock);
      if (error_cb)
        error_cb(session_id.c_str(), TIZENCLAW_ERROR_IO_ERROR,
                 "Failed to read body", user_data);
      return;
    }
    std::string resp_str(buf.data(), resp_len);
    if (response_cb)
      response_cb(session_id.c_str(), resp_str.c_str(), user_data);
  }

  close(sock);
}

}  // namespace

// --- C-API Public Definitions ---

API int tizenclaw_client_create(tizenclaw_client_h* client) {
  if (!client) {
    return TIZENCLAW_ERROR_INVALID_PARAMETER;
  }
  try {
    TizenClawClientImpl* impl = new TizenClawClientImpl();
    *client = static_cast<tizenclaw_client_h>(impl);
  } catch (const std::bad_alloc&) {
    return TIZENCLAW_ERROR_OUT_OF_MEMORY;
  }
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_client_destroy(tizenclaw_client_h client) {
  if (!client) {
    return TIZENCLAW_ERROR_INVALID_PARAMETER;
  }
  TizenClawClientImpl* impl = static_cast<TizenClawClientImpl*>(client);
  delete impl;
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_client_send_request(tizenclaw_client_h client,
                                  const char* session_id, const char* prompt,
                                  tizenclaw_response_cb response_cb,
                                  tizenclaw_error_cb error_cb,
                                  void* user_data) {
  if (!client || !prompt) {
    return TIZENCLAW_ERROR_INVALID_PARAMETER;
  }
  std::string s_id = session_id ? session_id : "default_session";
  std::string s_prompt = prompt;

  TizenClawClientImpl* impl = static_cast<TizenClawClientImpl*>(client);
  return impl->SendRequest(s_id, s_prompt, response_cb, error_cb, user_data);
}

API int tizenclaw_client_send_request_stream(tizenclaw_client_h client,
                                         const char* session_id,
                                         const char* prompt,
                                         tizenclaw_stream_cb stream_cb,
                                         tizenclaw_error_cb error_cb,
                                         void* user_data) {
  if (!client || !prompt) {
    return TIZENCLAW_ERROR_INVALID_PARAMETER;
  }
  std::string s_id = session_id ? session_id : "default_session";
  std::string s_prompt = prompt;

  TizenClawClientImpl* impl = static_cast<TizenClawClientImpl*>(client);
  return impl->SendRequestStream(s_id, s_prompt, stream_cb, error_cb,
                                 user_data);
}
