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
#ifndef CLI_SESSION_MANAGER_HH
#define CLI_SESSION_MANAGER_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <thread>
#include <atomic>
#include <chrono>

namespace tizenclaw::tool_executor {

enum class CliExecutionMode {
  kOneShot,
  kInteractive,
  kStreaming,
  kBackground,
  kPipe
};

struct CliSession {
  std::string session_id;
  std::string tool_name;
  CliExecutionMode mode;
  pid_t pid = -1;
  int stdin_fd = -1;
  int stdout_fd = -1;
  int stderr_fd = -1;
  std::chrono::steady_clock::time_point last_activity;
  int timeout_seconds = 60;
  bool active = true;
  std::string accumulated_output;
  mutable std::mutex mutex;

  CliSession() : last_activity(std::chrono::steady_clock::now()) {}
};

class CliSessionManager {
public:
  CliSessionManager();
  ~CliSessionManager();

  std::string CreateSession(const std::string& tool_path,
                            const std::string& arguments,
                            CliExecutionMode mode,
                            int timeout_seconds);

  std::string SendInput(const std::string& session_id,
                        const std::string& input,
                        int read_timeout_ms = 2000);

  std::string ReadOutput(const std::string& session_id,
                         int read_timeout_ms = 1000);

  std::string CloseSession(const std::string& session_id);

  nlohmann::json GetStatus(const std::string& session_id);
  nlohmann::json ListSessions();

private:
  std::string GenerateSessionId();
  void CleanupLoop();
  std::string ReadFromFd(int fd, int timeout_ms);

  std::map<std::string, std::unique_ptr<CliSession>> sessions_;
  mutable std::mutex sessions_mutex_;
  std::thread cleanup_thread_;
  std::atomic<bool> running_{true};
};

} // namespace tizenclaw::tool_executor

#endif // CLI_SESSION_MANAGER_HH
