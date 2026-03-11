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

#ifndef TIZENCLAW_PLUGIN_MANAGER_HH_
#define TIZENCLAW_PLUGIN_MANAGER_HH_

#include <memory>
#include <string>
#include <vector>
#include <mutex>
#include <map>
#include <functional>
#include "../infra/pkgmgr_client.hh"

namespace tizenclaw {

class PluginLlmBackend;

class PluginManager : public PkgmgrClient::IListener {
 public:
  static PluginManager& GetInstance();
  
  // Initialize the manager (start listening to pkgmgr events)
  bool Initialize();
  void Shutdown();

  // Get currently loaded plugin backends
  std::vector<std::shared_ptr<PluginLlmBackend>> GetLlmBackends();

  using ChangeCallback = std::function<void()>;
  void SetChangeCallback(ChangeCallback cb) { change_callback_ = cb; }

 private:
  PluginManager();
  ~PluginManager();

  // Disable copy
  PluginManager(const PluginManager&) = delete;
  PluginManager& operator=(const PluginManager&) = delete;

  void OnPkgmgrEvent(std::shared_ptr<PkgmgrEventArgs> args) override;

  void HandleInstallEvent(const std::string& pkgid);
  void HandleUpdateEvent(const std::string& pkgid);
  void HandleUninstallEvent(const std::string& pkgid);

  bool LoadPluginFromPkg(const std::string& pkgid);
  void UnloadPluginFromPkg(const std::string& pkgid);
  
  std::mutex map_mutex_;
  std::map<std::string, std::shared_ptr<PkgmgrEventArgs>> package_events_;

  std::mutex llm_backends_mutex_;
  std::vector<std::shared_ptr<PluginLlmBackend>> llm_backends_;
  ChangeCallback change_callback_;
};

} // namespace tizenclaw

#endif // TIZENCLAW_PLUGIN_MANAGER_HH_
