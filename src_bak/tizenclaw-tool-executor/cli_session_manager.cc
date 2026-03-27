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

#include "cli_session_manager.hh"

#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <cstring>
#include <ctime>
#include <iostream>
#include <vector>

#include "../common/logging.hh"

namespace tizenclaw::tool_executor {

CliSessionManager::CliSessionManager() {
  cleanup_thread_ = std::thread(&CliSessionManager::CleanupLoop, this);
}

CliSessionManager::~CliSessionManager() {
  running_ = false;
  if (cleanup_thread_.joinable()) {
    cleanup_thread_.join();
  }
}

std::string CliSessionManager::GenerateSessionId() {
  static std::atomic<int> counter{0};
  auto now = std::chrono::system_clock::now();
  auto ts = std::chrono::duration_cast<std::chrono::milliseconds>(
                now.time_since_epoch())
                .count();
  int seq = counter.fetch_add(1);
  return "cli_" + std::to_string(ts) + "_" + std::to_string(seq);
}

void CliSessionManager::CleanupLoop() {
  while (running_) {
    std::this_thread::sleep_for(std::chrono::seconds(10));
    std::lock_guard<std::mutex> lock(sessions_mutex_);
    auto now = std::chrono::steady_clock::now();
    for (auto it = sessions_.begin(); it != sessions_.end();) {
      auto& session = it->second;
      auto elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                         now - session->last_activity)
                         .count();
      if (elapsed > session->timeout_seconds || !session->active) {
        LOG(INFO) << "Cleaning up expired/inactive CLI session: "
                  << it->first;
        if (session->pid > 0) {
          kill(session->pid, SIGTERM);
          int status;
          waitpid(session->pid, &status, WNOHANG);
        }
        if (session->stdin_fd >= 0) close(session->stdin_fd);
        if (session->stdout_fd >= 0) close(session->stdout_fd);
        if (session->stderr_fd >= 0) close(session->stderr_fd);
        it = sessions_.erase(it);
      } else {
        ++it;
      }
    }
  }
}

std::string CliSessionManager::CreateSession(
    const std::string& tool_path,
    const std::string& arguments,
    CliExecutionMode mode,
    int timeout_seconds) {
  int in_pipe[2], out_pipe[2], err_pipe[2];
  if (pipe(in_pipe) < 0 || pipe(out_pipe) < 0 || pipe(err_pipe) < 0) {
    LOG(ERROR) << "pipe() failed: " << strerror(errno);
    return "";
  }

  pid_t pid = fork();
  if (pid < 0) {
    LOG(ERROR) << "fork() failed: " << strerror(errno);
    return "";
  }

  if (pid == 0) { // Child
    close(in_pipe[1]);
    close(out_pipe[0]);
    close(err_pipe[0]);

    dup2(in_pipe[0], STDIN_FILENO);
    dup2(out_pipe[1], STDOUT_FILENO);
    dup2(err_pipe[1], STDERR_FILENO);

    close(in_pipe[0]);
    close(out_pipe[1]);
    close(err_pipe[1]);

    // Split arguments
    std::vector<char*> args;
    args.push_back(const_cast<char*>(tool_path.c_str()));
    
    // Simple argument splitting (by space)
    std::string args_copy = arguments;
    char* token = strtok(const_cast<char*>(args_copy.c_str()), " ");
    while (token != nullptr) {
      args.push_back(token);
      token = strtok(nullptr, " ");
    }
    args.push_back(nullptr);

    execvp(tool_path.c_str(), args.data());
    _exit(1);
  }

  // Parent
  close(in_pipe[0]);
  close(out_pipe[1]);
  close(err_pipe[1]);

  // Set non-blocking
  fcntl(out_pipe[0], F_SETFL, O_NONBLOCK);
  fcntl(err_pipe[0], F_SETFL, O_NONBLOCK);

  auto session = std::make_unique<CliSession>();
  session->session_id = GenerateSessionId();
  session->tool_name = tool_path;
  session->mode = mode;
  session->pid = pid;
  session->stdin_fd = in_pipe[1];
  session->stdout_fd = out_pipe[0];
  session->stderr_fd = err_pipe[0];
  session->timeout_seconds = timeout_seconds;

  std::string sid = session->session_id;
  {
    std::lock_guard<std::mutex> lock(sessions_mutex_);
    sessions_[sid] = std::move(session);
  }

  LOG(INFO) << "Created CLI session " << sid << " pid=" << pid
            << " for " << tool_path;
  return sid;
}

std::string CliSessionManager::ReadFromFd(int fd, int timeout_ms) {
  std::string output;
  struct pollfd pfd = {fd, POLLIN, 0};
  
  while (true) {
    int ret = poll(&pfd, 1, timeout_ms);
    if (ret <= 0) break;

    char buf[4096];
    ssize_t n = read(fd, buf, sizeof(buf));
    if (n <= 0) break;
    output.append(buf, n);
    timeout_ms = 100; // Reduce subsequent timeout for continuous reading
  }
  return output;
}

std::string CliSessionManager::SendInput(
    const std::string& session_id,
    const std::string& input,
    int read_timeout_ms) {
  std::lock_guard<std::mutex> outer_lock(sessions_mutex_);
  auto it = sessions_.find(session_id);
  if (it == sessions_.end()) return "";

  auto& session = it->second;
  std::lock_guard<std::mutex> lock(session->mutex);
  session->last_activity = std::chrono::steady_clock::now();

  if (session->stdin_fd >= 0) {
    if (write(session->stdin_fd, input.data(), input.size()) < 0) {
      LOG(WARNING) << "write to stdin failed: " << strerror(errno);
    }
  }

  return ReadFromFd(session->stdout_fd, read_timeout_ms);
}

std::string CliSessionManager::ReadOutput(
    const std::string& session_id,
    int read_timeout_ms) {
  std::lock_guard<std::mutex> outer_lock(sessions_mutex_);
  auto it = sessions_.find(session_id);
  if (it == sessions_.end()) return "";

  auto& session = it->second;
  std::lock_guard<std::mutex> lock(session->mutex);
  session->last_activity = std::chrono::steady_clock::now();

  // Check if process is still alive
  int status;
  if (waitpid(session->pid, &status, WNOHANG) > 0) {
    session->active = false;
  }

  return ReadFromFd(session->stdout_fd, read_timeout_ms);
}

std::string CliSessionManager::CloseSession(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(sessions_mutex_);
  auto it = sessions_.find(session_id);
  if (it == sessions_.end()) return "Session not found";

  auto& session = it->second;
  if (session->pid > 0) {
    kill(session->pid, SIGTERM);
    int status;
    waitpid(session->pid, &status, 0); // Wait for term
  }
  
  session->active = false;
  // CleanupLoop will erase it from map. Mark as done.
  return "Session closed";
}

nlohmann::json CliSessionManager::GetStatus(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(sessions_mutex_);
  auto it = sessions_.find(session_id);
  if (it == sessions_.end()) return {{"status", "not_found"}};

  nlohmann::json j;
  j["session_id"] = session_id;
  j["tool_name"] = it->second->tool_name;
  j["active"] = it->second->active;
  return j;
}

nlohmann::json CliSessionManager::ListSessions() {
  std::lock_guard<std::mutex> lock(sessions_mutex_);
  nlohmann::json arr = nlohmann::json::array();
  for (const auto& [id, session] : sessions_) {
    arr.push_back({{"id", id},
                   {"tool", session->tool_name},
                   {"active", session->active}});
  }
  return arr;
}

} // namespace tizenclaw::tool_executor
