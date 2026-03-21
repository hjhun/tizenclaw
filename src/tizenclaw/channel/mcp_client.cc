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

#include "mcp_client.hh"
#include "../../common/logging.hh"

#include <fcntl.h>
#include <poll.h>
#include <sys/wait.h>
#include <unistd.h>
#include <iostream>
#include <thread>

namespace tizenclaw {

McpClient::McpClient(const std::string& server_name, const std::string& command,
                     const std::vector<std::string>& args, int timeout_ms)
    : server_name_(server_name), command_(command), args_(args), timeout_ms_(timeout_ms) {
    UpdateLastUsed();
}

McpClient::~McpClient() { Disconnect(); }

void McpClient::UpdateLastUsed() {
  last_used_ms_ = std::chrono::duration_cast<std::chrono::milliseconds>(
                      std::chrono::system_clock::now().time_since_epoch())
                      .count();
}

long long McpClient::GetLastUsedMs() const {
  return last_used_ms_;
}

bool McpClient::Connect() {
  if (is_connected_) return true;
  UpdateLastUsed();

  if (pipe(pipe_stdin_) == -1 || pipe(pipe_stdout_) == -1) {
    LOG(ERROR) << "MCP Client: Failed to create pipes for " << server_name_;
    return false;
  }

  // Make read end non-blocking
  int flags = fcntl(pipe_stdout_[0], F_GETFL, 0);
  fcntl(pipe_stdout_[0], F_SETFL, flags | O_NONBLOCK);

  pid_ = fork();
  if (pid_ < 0) {
    LOG(ERROR) << "MCP Client: Fork failed for " << server_name_;
    return false;
  }

  if (pid_ == 0) {
    // Child process: set up stdin/stdout
    dup2(pipe_stdin_[0], STDIN_FILENO);
    dup2(pipe_stdout_[1], STDOUT_FILENO);

    close(pipe_stdin_[1]);
    close(pipe_stdout_[0]);
    close(pipe_stdin_[0]);
    close(pipe_stdout_[1]);

    // Build args array
    std::vector<const char*> exec_args;
    exec_args.push_back(command_.c_str());
    for (const auto& arg : args_) {
      exec_args.push_back(arg.c_str());
    }
    exec_args.push_back(nullptr);

    execvp(command_.c_str(), const_cast<char* const*>(exec_args.data()));
    // If exec fails, exit child silently to avoid polluting parent logs
    _exit(1);
  }

  // Parent process
  close(pipe_stdin_[0]);
  close(pipe_stdout_[1]);
  is_connected_ = true;

  LOG(INFO) << "MCP Client: Process for " << server_name_ << " started (PID: " << pid_ << ")";

  // Perform 'initialize' handshake
  try {
    nlohmann::json init_params = {
        {"protocolVersion", "2024-11-05"},
        {"capabilities", nlohmann::json::object()},
        {"clientInfo", {{"name", "tizenclaw-mcp-client"}, {"version", "1.0.0"}}}
    };
    
    nlohmann::json response = SendRequestSync("initialize", init_params, 10000);
    if (response.contains("error")) {
      LOG(ERROR) << "MCP Client: Handshake failed for " << server_name_;
      Disconnect();
      return false;
    }
    
    // We must send notifications/initialized next
    nlohmann::json notif = {
      {"jsonrpc", "2.0"},
      {"method", "notifications/initialized"}
    };
    SendRpcMessage(notif);
    
    LOG(INFO) << "MCP Client: Handshake succeeded for " << server_name_;

  } catch (const std::exception& e) {
    LOG(ERROR) << "MCP Client: Initialization exception for " << server_name_ << ": " << e.what();
    Disconnect();
    return false;
  }

  return true;
}

void McpClient::Disconnect() {
  if (!is_connected_) return;

  is_connected_ = false;
  
  if (pipe_stdin_[1] != -1) close(pipe_stdin_[1]);
  if (pipe_stdout_[0] != -1) close(pipe_stdout_[0]);

  if (pid_ > 0) {
    kill(pid_, SIGTERM);
    int wstatus;
    int ret = 0;
    for (int i = 0; i < 10; ++i) {
      ret = waitpid(pid_, &wstatus, WNOHANG);
      if (ret > 0) break;
      std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    if (ret <= 0) {
      LOG(WARNING) << "MCP Client: " << server_name_ << " stuck, sending SIGKILL";
      kill(pid_, SIGKILL);
      waitpid(pid_, &wstatus, 0);
    }
    pid_ = -1;
  }
}

std::vector<McpClient::ToolInfo> McpClient::GetTools() {
  std::vector<ToolInfo> tools;
  if (!is_connected_) return tools;

  try {
    nlohmann::json response = SendRequestSync("tools/list", nlohmann::json::object(), 5000);
    if (response.contains("result") && response["result"].contains("tools")) {
      for (const auto& t : response["result"]["tools"]) {
        ToolInfo info;
        info.name = t.value("name", "");
        info.description = t.value("description", "");
        if (t.contains("inputSchema")) {
          info.input_schema = t["inputSchema"];
        }
        tools.push_back(info);
      }
    }
  } catch (const std::exception& e) {
    LOG(ERROR) << "MCP Client: GetTools error for " << server_name_ << ": " << e.what();
  }
  return tools;
}

nlohmann::json McpClient::CallTool(const std::string& tool_name, const nlohmann::json& arguments) {
  if (!is_connected_) return {{"error", "Not connected"}};

  nlohmann::json params = {
    {"name", tool_name},
    {"arguments", arguments}
  };

  UpdateLastUsed();

  try {
    nlohmann::json response = SendRequestSync("tools/call", params, timeout_ms_);
    LOG(INFO) << "MCP Client: CallTool response for " << tool_name << ": " << response.dump();
    if (response.contains("result")) {
      return response["result"];
    } else if (response.contains("error")) {
      LOG(WARNING) << "MCP Client: CallTool error from " << server_name_ << ": " << response["error"].dump();
      return {{"isError", true}, {"error", response["error"]}};
    }
    return {{"isError", true}, {"error", "Invalid response from server"}};
  } catch (const std::exception& e) {
    LOG(ERROR) << "MCP Client: CallTool exception for " << server_name_ << ": " << e.what();
    return {{"isError", true}, {"error", e.what()}};
  }
}

bool McpClient::SendRpcMessage(const nlohmann::json& message) {
  std::lock_guard<std::mutex> lock(io_mutex_);
  if (!is_connected_) return false;

  std::string data = message.dump() + "\n";
  ssize_t written = write(pipe_stdin_[1], data.c_str(), data.length());
  return written == static_cast<ssize_t>(data.length());
}

nlohmann::json McpClient::ReadRpcMessage(int timeout_ms) {
  if (!is_connected_) return nullptr;

  auto parse_line = [this]() -> nlohmann::json {
    size_t pos = read_buffer_.find('\n');
    if (pos != std::string::npos) {
      std::string line = read_buffer_.substr(0, pos);
      read_buffer_.erase(0, pos + 1);
      if (line.empty() || line == "\r") return nlohmann::json(nullptr);
      try {
        return nlohmann::json::parse(line);
      } catch (...) {
        return nlohmann::json(nullptr);
      }
    }
    return nlohmann::json(nullptr);
  };

  // Try parsing any full lines already in buffer
  nlohmann::json parsed = parse_line();
  if (!parsed.is_null()) return parsed;

  struct pollfd pfd;
  pfd.fd = pipe_stdout_[0];
  pfd.events = POLLIN;

  char chunk[4096];

  auto start_time = std::chrono::steady_clock::now();
  
  while (is_connected_) {
    auto now = std::chrono::steady_clock::now();
    int elapsed_ms = std::chrono::duration_cast<std::chrono::milliseconds>(now - start_time).count();
    int remaining_ms = std::max(1, timeout_ms - elapsed_ms);
    if (elapsed_ms >= timeout_ms) break; // Timeout

    int ret = poll(&pfd, 1, remaining_ms);
    if (ret < 0) {
      if (errno == EINTR) continue;
      break; 
    }
    if (ret == 0) break; // Timeout

    if (pfd.revents & POLLIN || pfd.revents & POLLHUP) {
      ssize_t bytes = read(pipe_stdout_[0], chunk, sizeof(chunk) - 1);
      if (bytes > 0) {
        chunk[bytes] = '\0';
        read_buffer_.append(chunk, bytes);
        
        parsed = parse_line();
        if (!parsed.is_null()) return parsed;
      } else if (bytes == 0) {
        // EOF
        LOG(WARNING) << "MCP Client: Server closed pipe for " << server_name_;
        Disconnect();
        break;
      } else {
        if (errno != EAGAIN && errno != EWOULDBLOCK) {
          Disconnect();
          break;
        }
      }
    }
  }
  return nlohmann::json(nullptr); // Timeout
}

nlohmann::json McpClient::SendRequestSync(const std::string& method, const nlohmann::json& params, int timeout_ms) {
  int req_id = next_req_id_++;
  nlohmann::json request = {
    {"jsonrpc", "2.0"},
    {"id", req_id},
    {"method", method},
    {"params", params}
  };

  if (!SendRpcMessage(request)) {
    throw std::runtime_error("Failed to send request");
  }

  // Poll loop: wait for ID match
  auto start_time = std::chrono::steady_clock::now();
  while (is_connected_) {
    auto now = std::chrono::steady_clock::now();
    int elapsed_ms = std::chrono::duration_cast<std::chrono::milliseconds>(now - start_time).count();
    int remaining_ms = std::max(1, timeout_ms - elapsed_ms);
    
    if (elapsed_ms >= timeout_ms) break;

    nlohmann::json res = ReadRpcMessage(remaining_ms);
    if (res.is_null()) continue; 
    if (res.is_discarded()) continue; // Parse error

    if (res.contains("id") && res["id"] == req_id) {
      return res;
    }
    
    // Non-matching IDs handled here
    if (res.contains("method")) {
      LOG(INFO) << "MCP Client: Notification from " << server_name_ << " -> " << res["method"];
    }
  }

  throw std::runtime_error("Server request timed out after " + std::to_string(timeout_ms) + "ms");
}

} // namespace tizenclaw
