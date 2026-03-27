/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 */

#include "canvas_ipc_server.hh"

#include <dlog.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>
#include <json.hpp>

#include <algorithm>
#include <cstring>

#ifndef LOG_TAG
#define LOG_TAG "TIZENCLAW"
#endif

namespace tizenclaw {

constexpr const char* CanvasIpcServer::kSocketPath;

CanvasIpcServer::CanvasIpcServer() = default;

CanvasIpcServer::~CanvasIpcServer() {
  Stop();
}

bool CanvasIpcServer::Start() {
  if (running_.load()) return true;

  // Cleanup old socket file just in case
  unlink(kSocketPath);

  server_fd_ = socket(AF_UNIX, SOCK_STREAM | O_CLOEXEC, 0);
  if (server_fd_ < 0) {
    LOGE("Failed to create canvas IPC socket");
    return false;
  }

  // Bind
  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, kSocketPath, sizeof(addr.sun_path) - 1);

  if (bind(server_fd_, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
    LOGE("Failed to bind canvas IPC socket: %s", strerror(errno));
    close(server_fd_);
    server_fd_ = -1;
    return false;
  }

  // Set permissions so Canvas App (could be any UID) can write/connect
  chmod(kSocketPath, 0666);

  if (listen(server_fd_, 5) < 0) {
    LOGE("Failed to listen on canvas IPC socket: %s", strerror(errno));
    close(server_fd_);
    server_fd_ = -1;
    return false;
  }

  running_.store(true);
  server_thread_ = std::thread(&CanvasIpcServer::ServerLoop, this);
  LOGI("Canvas IPC Server started successfully at %s", kSocketPath);
  return true;
}

void CanvasIpcServer::Stop() {
  if (!running_.load()) return;

  running_.store(false);

  // Close server socket to interrupt accept()
  if (server_fd_ >= 0) {
    shutdown(server_fd_, SHUT_RDWR);
    close(server_fd_);
    server_fd_ = -1;
  }

  if (server_thread_.joinable()) {
    server_thread_.join();
  }

  unlink(kSocketPath);

  // Close connected clients
  std::lock_guard<std::mutex> lock(clients_mutex_);
  for (int fd : client_fds_) {
    close(fd);
  }
  client_fds_.clear();

  LOGI("Canvas IPC Server stopped");
}

void CanvasIpcServer::ServerLoop() {
  while (running_.load()) {
    struct sockaddr_un client_addr;
    socklen_t client_len = sizeof(client_addr);
    
    int client_fd = accept(server_fd_, (struct sockaddr*)&client_addr, &client_len);
    if (client_fd < 0) {
      if (running_.load()) {
        LOGE("Canvas IPC accept error: %s", strerror(errno));
      }
      break;
    }

    // Add to connected clients
    {
      std::lock_guard<std::mutex> lock(clients_mutex_);
      client_fds_.push_back(client_fd);
    }
    LOGI("Canvas app connected: fd %d", client_fd);
    
    // Send initial status
    BroadcastState("idle", "TizenClaw Engine Connected");
  }
}

void CanvasIpcServer::BroadcastState(const std::string& state, const std::string& content) {
  if (!running_.load()) return;

  nlohmann::json root = {
      {"type", "state"},
      {"state", state},
      {"content", content}
  };

  std::string msg = root.dump() + "\n";
  const char* p = msg.c_str();
  size_t len = msg.length();

  std::lock_guard<std::mutex> lock(clients_mutex_);
  std::vector<int> dead_clients;

  for (int fd : client_fds_) {
    ssize_t sent = send(fd, p, len, MSG_NOSIGNAL);
    if (sent < 0 && (errno == EPIPE || errno == ECONNRESET)) {
      dead_clients.push_back(fd);
    }
  }

  // Cleanup dead sockets
  for (int fd : dead_clients) {
    close(fd);
    auto it = std::find(client_fds_.begin(), client_fds_.end(), fd);
    if (it != client_fds_.end()) {
      client_fds_.erase(it);
    }
    LOGI("Canvas app disconnected: fd %d", fd);
  }
}

}  // namespace tizenclaw
