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

#ifndef TIZENCLAW_CLI_SOCKET_CLIENT_HH_
#define TIZENCLAW_CLI_SOCKET_CLIENT_HH_

#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

class SocketClient {
 public:
  // Send a JSON-RPC request and return the raw
  // response body string.
  [[nodiscard]] std::string SendJsonRpc(
      const std::string& method,
      const std::string& params = "{}");

  // Send outbound message via channel.
  [[nodiscard]] int SendToChannel(
      const std::string& channel,
      const std::string& text);

  // Send request directly to tool executor
  [[nodiscard]] std::string SendToExecutor(
      const std::string& tool,
      const std::string& args);

 private:
  // Connect to the tizenclaw daemon abstract socket.
  // Returns fd >= 0 on success, -1 on failure.
  [[nodiscard]] int Connect() const;

  // Connect to the tizenclaw-tool-executor abstract socket.
  [[nodiscard]] int ConnectToExecutor() const;

  // Write length-prefixed payload to fd.
  [[nodiscard]] bool SendPayload(int fd,
      const std::string& payload) const;

  // Read length-prefixed response from fd.
  [[nodiscard]] std::string RecvResponse(int fd) const;

  static constexpr const char kSocketName[] =
      "tizenclaw.sock";

  static constexpr const char kExecutorSocketName[] =
      "tizenclaw-tool-executor.sock";
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_SOCKET_CLIENT_HH_
