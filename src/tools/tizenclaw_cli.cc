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
 * tizenclaw-cli: CLI tool for testing TizenClaw daemon via UDS IPC.
 *
 * Usage:
 *   tizenclaw-cli "What is the battery level?"
 *   tizenclaw-cli -s my_session "Run a skill"
 *   tizenclaw-cli --stream "Tell me about Tizen"
 *   tizenclaw-cli   (interactive mode)
 */

#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <cstring>
#include <iostream>
#include <string>
#include <vector>

// Minimal JSON serialization (no external deps)
namespace {

std::string JsonEscape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 8);
  for (char c : s) {
    switch (c) {
      case '"':  out += "\\\""; break;
      case '\\': out += "\\\\"; break;
      case '\n': out += "\\n";  break;
      case '\r': out += "\\r";  break;
      case '\t': out += "\\t";  break;
      default:   out += c;      break;
    }
  }
  return out;
}

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
    std::cerr << "Error: socket() failed: "
              << strerror(errno) << "\n";
    return -1;
  }

  struct sockaddr_un addr;
  std::memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  // Abstract namespace: \0tizenclaw.ipc
  const char kName[] = "tizenclaw.ipc";
  std::memcpy(addr.sun_path + 1, kName,
              sizeof(kName) - 1);
  socklen_t addr_len =
      offsetof(struct sockaddr_un, sun_path) +
      1 + sizeof(kName) - 1;

  if (connect(sock,
              reinterpret_cast<struct sockaddr*>(&addr),
              addr_len) < 0) {
    std::cerr << "Error: connect(\\0tizenclaw.ipc) failed: "
              << strerror(errno) << "\n"
              << "Is *tizenclaw.service* running?\n";
    close(sock);
    return -1;
  }
  return sock;
}

// Send request and receive response
// Returns the response body string, or "" on error.
std::string SendRequest(int sock,
                        const std::string& session_id,
                        const std::string& prompt,
                        bool stream) {
  // Build JSON manually (no nlohmann dependency)
  std::string json_req =
      "{\"session_id\":\"" + JsonEscape(session_id) +
      "\",\"text\":\"" + JsonEscape(prompt) +
      "\",\"stream\":" + (stream ? "true" : "false") +
      "}";

  // Send length-prefixed request
  uint32_t net_len = htonl(
      static_cast<uint32_t>(json_req.size()));
  if (!SendAll(sock, &net_len, 4) ||
      !SendAll(sock, json_req.data(),
               json_req.size())) {
    std::cerr << "Error: failed to send request\n";
    return "";
  }

  if (stream) {
    // Read streaming chunks until stream_end
    std::string final_text;
    while (true) {
      uint32_t resp_net_len = 0;
      if (!RecvExact(sock, &resp_net_len, 4)) {
        std::cerr << "\nError: recv header failed\n";
        break;
      }
      uint32_t resp_len = ntohl(resp_net_len);
      if (resp_len > 10 * 1024 * 1024) {
        std::cerr << "\nError: response too large\n";
        break;
      }
      std::vector<char> buf(resp_len);
      if (!RecvExact(sock, buf.data(), resp_len)) {
        std::cerr << "\nError: recv body failed\n";
        break;
      }
      std::string chunk_str(buf.data(), resp_len);

      // Simple JSON parsing for "type" and "text"
      auto find_value = [](const std::string& json,
                           const std::string& key) -> std::string {
        std::string needle = "\"" + key + "\":\"";
        auto pos = json.find(needle);
        if (pos == std::string::npos) return "";
        pos += needle.size();
        std::string val;
        for (size_t i = pos; i < json.size(); ++i) {
          if (json[i] == '"' &&
              (i == 0 || json[i-1] != '\\')) break;
          val += json[i];
        }
        return val;
      };

      std::string type = find_value(chunk_str, "type");
      std::string text = find_value(chunk_str, "text");

      if (type == "stream_chunk") {
        std::cout << text << std::flush;
      } else if (type == "stream_end") {
        if (!text.empty()) {
          final_text = text;
        }
        std::cout << "\n";
        break;
      } else {
        // Unexpected — print raw
        final_text = chunk_str;
        break;
      }
    }
    return final_text;
  }

  // Non-streaming: single response
  uint32_t resp_net_len = 0;
  if (!RecvExact(sock, &resp_net_len, 4)) {
    std::cerr << "Error: recv header failed\n";
    return "";
  }
  uint32_t resp_len = ntohl(resp_net_len);
  if (resp_len > 10 * 1024 * 1024) {
    std::cerr << "Error: response too large: "
              << resp_len << "\n";
    return "";
  }
  std::vector<char> buf(resp_len);
  if (!RecvExact(sock, buf.data(), resp_len)) {
    std::cerr << "Error: recv body incomplete\n";
    return "";
  }
  return std::string(buf.data(), resp_len);
}

void PrintUsage() {
  std::cerr
      << "tizenclaw-cli — TizenClaw IPC test tool\n\n"
      << "Usage:\n"
      << "  tizenclaw-cli [options] [prompt]\n\n"
      << "Options:\n"
      << "  -s <id>     Session ID (default: cli_test)\n"
      << "  --stream    Enable streaming mode\n"
      << "  -h, --help  Show this help\n\n"
      << "If no prompt is given, enters interactive mode.\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  std::string session_id = "cli_test";
  bool stream = false;
  std::string prompt;

  // Parse args
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
      // Remaining args are the prompt
      for (int j = i; j < argc; ++j) {
        if (!prompt.empty()) prompt += " ";
        prompt += argv[j];
      }
      break;
    }
  }

  // Single-shot mode
  if (!prompt.empty()) {
    int sock = ConnectToSocket();
    if (sock < 0) return 1;

    std::string resp = SendRequest(
        sock, session_id, prompt, stream);
    close(sock);

    if (resp.empty()) return 1;
    if (!stream) {
      std::cout << resp << "\n";
    }
    return 0;
  }

  // Interactive mode
  std::cout << "TizenClaw CLI (session: "
            << session_id << ")\n"
            << "Type a prompt and press Enter. "
            << "Ctrl+D to exit.\n\n";

  int sock = ConnectToSocket();
  if (sock < 0) return 1;

  while (true) {
    std::cout << "you> " << std::flush;
    std::string line;
    if (!std::getline(std::cin, line)) break;
    if (line.empty()) continue;

    std::string resp = SendRequest(
        sock, session_id, line, stream);
    if (resp.empty()) {
      std::cerr << "(connection lost, reconnecting...)\n";
      close(sock);
      sock = ConnectToSocket();
      if (sock < 0) return 1;
      continue;
    }
    if (!stream) {
      std::cout << "\nassistant> " << resp << "\n\n";
    }
  }

  close(sock);
  std::cout << "\nBye.\n";
  return 0;
}
