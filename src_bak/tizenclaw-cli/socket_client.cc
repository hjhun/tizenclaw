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

#include "socket_client.hh"

#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cstring>
#include <iostream>
#include <vector>

#include <nlohmann/json.hpp>

namespace tizenclaw {
namespace cli {

int SocketClient::Connect() const {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    std::cerr << "Failed to create socket\n";
    return -1;
  }

  struct sockaddr_un addr = {};
  addr.sun_family = AF_UNIX;
  for (size_t i = 0; i < sizeof(kSocketName) - 1; ++i)
    addr.sun_path[1 + i] = kSocketName[i];
  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path)
      + 1 + sizeof(kSocketName) - 1;

  if (connect(sock,
              reinterpret_cast<struct sockaddr*>(
                  &addr),
              addr_len) < 0) {
    close(sock);
    std::cerr << "Failed to connect to daemon\n";
    return -1;
  }
  return sock;
}

int SocketClient::ConnectToExecutor() const {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    std::cerr << "Failed to create socket\n";
    return -1;
  }

  struct sockaddr_un addr = {};
  addr.sun_family = AF_UNIX;
  for (size_t i = 0; i < sizeof(kExecutorSocketName) - 1; ++i)
    addr.sun_path[1 + i] = kExecutorSocketName[i];
  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path)
      + 1 + sizeof(kExecutorSocketName) - 1;

  if (connect(sock,
              reinterpret_cast<struct sockaddr*>(
                  &addr),
              addr_len) < 0) {
    close(sock);
    std::cerr << "Failed to connect to tool executor\n";
    return -1;
  }
  return sock;
}

bool SocketClient::SendPayload(
    int fd, const std::string& payload) const {
  uint32_t net_len = htonl(payload.size());
  if (write(fd, &net_len, 4) != 4) return false;
  ssize_t total = 0;
  ssize_t len = static_cast<ssize_t>(payload.size());
  while (total < len) {
    ssize_t w = write(fd, payload.data() + total,
                      len - total);
    if (w <= 0) return false;
    total += w;
  }
  return true;
}

std::string SocketClient::RecvResponse(int fd) const {
  uint32_t resp_len = 0;
  if (read(fd, &resp_len, 4) != 4) return "";
  resp_len = ntohl(resp_len);
  std::vector<char> buf(resp_len);
  size_t got = 0;
  while (got < resp_len) {
    auto r = read(fd, buf.data() + got,
                  resp_len - got);
    if (r <= 0) break;
    got += r;
  }
  return std::string(buf.data(), got);
}

std::string SocketClient::SendJsonRpc(
    const std::string& method,
    const std::string& params) {
  int sock = Connect();
  if (sock < 0) return "";

  std::string req =
      "{\"jsonrpc\":\"2.0\",\"method\":\""
      + method + "\",\"id\":1,\"params\":"
      + params + "}";

  if (!SendPayload(sock, req)) {
    close(sock);
    std::cerr << "Failed to send request\n";
    return "";
  }

  std::string resp = RecvResponse(sock);
  close(sock);
  return resp;
}

int SocketClient::SendToChannel(
    const std::string& channel,
    const std::string& text) {
  int sock = Connect();
  if (sock < 0) return 1;

  // Build JSON-RPC request with escaped text
  std::string escaped_text;
  for (char c : text) {
    if (c == '"') escaped_text += "\\\"";
    else if (c == '\\') escaped_text += "\\\\";
    else if (c == '\n') escaped_text += "\\n";
    else escaped_text += c;
  }

  std::string req =
      "{\"jsonrpc\":\"2.0\",\"method\":"
      "\"send_to\",\"id\":1,\"params\":{"
      "\"channel\":\"" + channel + "\","
      "\"text\":\"" + escaped_text + "\"}}";

  if (!SendPayload(sock, req)) {
    close(sock);
    std::cerr << "Failed to send request\n";
    return 1;
  }

  std::string resp = RecvResponse(sock);
  close(sock);

  if (!resp.empty()) {
    std::cout << resp << "\n";
  }
  return 0;
}

std::string SocketClient::SendToExecutor(
    const std::string& command,
    const std::string& params_json) {
  int sock = ConnectToExecutor();
  if (sock < 0) return "";

  nlohmann::json req;
  try {
    if (!params_json.empty() && params_json != "{}") {
      req = nlohmann::json::parse(params_json);
    }
  } catch (const std::exception& e) {
    std::cerr << "Failed to parse params_json: " << e.what() << "\n";
    // Fallback or treat as empty
  }
  
  req["command"] = command;

  if (!SendPayload(sock, req.dump())) {
    close(sock);
    std::cerr << "Failed to send request to executor\n";
    return "";
  }

  std::string resp = RecvResponse(sock);
  close(sock);
  return resp;
}

}  // namespace cli
}  // namespace tizenclaw
