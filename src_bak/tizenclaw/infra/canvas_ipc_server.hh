/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 */

#ifndef CANVAS_IPC_SERVER_HH
#define CANVAS_IPC_SERVER_HH

#include <atomic>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace tizenclaw {

class CanvasIpcServer {
 public:
  CanvasIpcServer();
  ~CanvasIpcServer();

  // Non-copyable/movable
  CanvasIpcServer(const CanvasIpcServer&) = delete;
  CanvasIpcServer& operator=(const CanvasIpcServer&) = delete;

  // Initialize and start the UDS server thread
  bool Start();

  // Stop the server
  void Stop();

  // Broadcast agent state to connected Canvas clients
  void BroadcastState(const std::string& state, const std::string& content);

 private:
  void ServerLoop();
  void RemoveClient(int fd);

  int server_fd_ = -1;
  std::atomic<bool> running_{false};
  std::thread server_thread_;

  std::mutex clients_mutex_;
  std::vector<int> client_fds_;

  static constexpr const char* kSocketPath = "/run/tizenclaw/canvas.sock";
};

}  // namespace tizenclaw

#endif  // CANVAS_IPC_SERVER_HH
