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
#ifndef TIZENCLAW_HH
#define TIZENCLAW_HH

#include <tizen_core.h>

#include <algorithm>
#include <atomic>
#include <json.hpp>
#include <mutex>
#include <span>
#include <thread>
#include <vector>

#include "../../common/logging.hh"
#include "../channel/channel_factory.hh"
#include "../channel/channel_registry.hh"
#include "../channel/mcp_server.hh"
#include "../scheduler/task_scheduler.hh"
#include "agent_core.hh"
#include "event_bus.hh"
#include "event_adapter_manager.hh"
#include "system_event_collector.hh"
#include "autonomous_trigger.hh"
#include "perception_engine.hh"
#include "skill_repository.hh"
#include "skill_watcher.hh"
#include "../infra/fleet_agent.hh"

namespace tizenclaw {

class TizenClawDaemon {
 public:
  TizenClawDaemon(int argc, char** argv);
  ~TizenClawDaemon();

  int Run();
  void Quit();

 private:
  void OnCreate();
  void OnDestroy();
  void IpcServerLoop();
  void HandleIpcClient(int client_sock);
  [[nodiscard]] bool IsAllowedUid(uid_t uid) const;

  int argc_;
  char** argv_;
  tizen_core_task_h task_ = nullptr;
  std::unique_ptr<AgentCore> agent_;

  std::thread ipc_thread_;
  int ipc_socket_;
  bool ipc_running_;
  std::unique_ptr<TaskScheduler> scheduler_;
  ChannelRegistry channel_registry_;
  SkillWatcher skill_watcher_;
  std::unique_ptr<SystemEventCollector> event_collector_;
  EventAdapterManager adapter_manager_;
  std::unique_ptr<AutonomousTrigger> auto_trigger_;
  std::unique_ptr<PerceptionEngine> perception_engine_;
  std::unique_ptr<SkillRepository> skill_repo_;
  std::unique_ptr<FleetAgent> fleet_agent_;

  // Concurrency control
  std::atomic<int> active_clients_{0};
  static constexpr int kMaxConcurrentClients = 4;

  // Allowed UIDs for IPC connections
  // 0=root, 301=app_fw, 200=system, 5001=developer
  static constexpr uid_t kAllowedUids[] = {0, 200, 301, 5001};
};

}  // namespace tizenclaw

#endif  // TIZENCLAW_HH
