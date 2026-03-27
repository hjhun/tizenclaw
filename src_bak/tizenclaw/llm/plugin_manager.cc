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

#include "plugin_manager.hh"

#include <pkgmgr-info.h>

#include <fstream>
#include <thread>

#include "../../common/logging.hh"
#include "plugin_llm_backend.hh"

namespace tizenclaw {

PluginManager& PluginManager::GetInstance() {
  static PluginManager instance;
  return instance;
}

PluginManager::PluginManager() {}

PluginManager::~PluginManager() { Shutdown(); }

bool PluginManager::Initialize() {
  PkgmgrClient::GetInstance().AddListener(this);

  pkgmgrinfo_pkginfo_metadata_filter_h filter;
  int ret = pkgmgrinfo_pkginfo_metadata_filter_create(&filter);
  if (ret != PMINFO_R_OK) {
    LOG(ERROR) << "Failed to create metadata filter: " << ret;
    return true;  // Graceful fallback for headless unit tests
  }

  pkgmgrinfo_pkginfo_metadata_filter_add(
      filter, "http://tizen.org/metadata/tizenclaw/llm-backend", nullptr);

  pkgmgrinfo_pkginfo_metadata_filter_foreach(
      filter,
      [](pkgmgrinfo_pkginfo_h handle, void* user_data) {
        auto* manager = static_cast<PluginManager*>(user_data);
        char* pkgid = nullptr;
        if (pkgmgrinfo_pkginfo_get_pkgid(handle, &pkgid) == PMINFO_R_OK &&
            pkgid) {
          manager->LoadPluginFromPkg(pkgid);
        }
        return 0;
      },
      this);

  pkgmgrinfo_pkginfo_metadata_filter_destroy(filter);

  // Scan for channel plugins
  pkgmgrinfo_pkginfo_metadata_filter_h ch_filter;
  ret = pkgmgrinfo_pkginfo_metadata_filter_create(
      &ch_filter);
  if (ret == PMINFO_R_OK) {
    pkgmgrinfo_pkginfo_metadata_filter_add(
        ch_filter,
        "http://tizen.org/metadata/"
        "tizenclaw/channel",
        nullptr);

    pkgmgrinfo_pkginfo_metadata_filter_foreach(
        ch_filter,
        [](pkgmgrinfo_pkginfo_h handle,
           void* user_data) {
          auto* mgr =
              static_cast<PluginManager*>(user_data);
          char* pkgid = nullptr;
          if (pkgmgrinfo_pkginfo_get_pkgid(
                  handle, &pkgid) == PMINFO_R_OK &&
              pkgid) {
            mgr->LoadChannelPluginFromPkg(pkgid);
          }
          return 0;
        },
        this);

    pkgmgrinfo_pkginfo_metadata_filter_destroy(
        ch_filter);
  }

  return true;
}

void PluginManager::Shutdown() {
  PkgmgrClient::GetInstance().RemoveListener(this);

  {
    std::lock_guard<std::mutex> lock(
        llm_backends_mutex_);
    for (auto& backend : llm_backends_)
      backend->Shutdown();
    llm_backends_.clear();
  }

  {
    std::lock_guard<std::mutex> lock(
        channel_plugins_mutex_);
    for (auto& ch : channel_plugins_)
      ch->Stop();
    channel_plugins_.clear();
  }
}

std::vector<std::shared_ptr<PluginLlmBackend>>
PluginManager::GetLlmBackends() const {
  std::lock_guard<std::mutex> lock(
      llm_backends_mutex_);
  return llm_backends_;
}

std::vector<std::shared_ptr<PluginChannel>>
PluginManager::GetChannelPlugins() const {
  std::lock_guard<std::mutex> lock(
      channel_plugins_mutex_);
  return channel_plugins_;
}

void PluginManager::OnPkgmgrEvent(std::shared_ptr<PkgmgrEventArgs> args) {
  if (args->GetEventStatus() == "start") {
    std::lock_guard<std::mutex> lock(map_mutex_);
    package_events_[args->GetTag()] = args;
  } else if (args->GetEventStatus() == "end" && args->GetEventName() == "ok") {
    std::shared_ptr<PkgmgrEventArgs> start_event;
    {
      std::lock_guard<std::mutex> lock(map_mutex_);
      auto it = package_events_.find(args->GetTag());
      if (it != package_events_.end()) {
        start_event = it->second;
        package_events_.erase(it);
      }
    }

    if (start_event) {
      const auto& evt = start_event->GetEventName();
      if (evt == "install" || evt == "recoverinstall") {
        HandleInstallEvent(args->GetPkgId());
      } else if (evt == "upgrade" || evt == "recoverupgrade") {
        HandleUpdateEvent(args->GetPkgId());
      } else if (evt == "uninstall" || evt == "recoveruninstall") {
        HandleUninstallEvent(args->GetPkgId());
      }
    }
  } else if (args->GetEventStatus() == "error") {
    std::lock_guard<std::mutex> lock(map_mutex_);
    package_events_.erase(args->GetTag());
  }
}

void PluginManager::HandleInstallEvent(
    const std::string& pkgid) {
  LOG(INFO) << "Install event for " << pkgid;
  LoadPluginFromPkg(pkgid);
  LoadChannelPluginFromPkg(pkgid);
}

void PluginManager::HandleUpdateEvent(
    const std::string& pkgid) {
  LOG(INFO) << "Update event for " << pkgid;
  UnloadPluginFromPkg(pkgid);
  UnloadChannelPluginFromPkg(pkgid);
  LoadPluginFromPkg(pkgid);
  LoadChannelPluginFromPkg(pkgid);
}

void PluginManager::HandleUninstallEvent(
    const std::string& pkgid) {
  LOG(INFO) << "Uninstall event for " << pkgid;
  UnloadPluginFromPkg(pkgid);
  UnloadChannelPluginFromPkg(pkgid);
}

bool PluginManager::LoadPluginFromPkg(const std::string& pkgid) {
  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret =
      pkgmgrinfo_pkginfo_get_usr_pkginfo(pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) {
    LOG(ERROR) << "Failed to get pkginfo for " << pkgid;
    return false;
  }

  char* res_path = nullptr;
  ret = pkgmgrinfo_pkginfo_get_root_path(pkginfo, &res_path);
  if (ret != PMINFO_R_OK || !res_path) {
    LOG(ERROR) << "Failed to get root path for " << pkgid;
    pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
    return false;
  }

  std::string path(res_path);

  char* so_value = nullptr;
  ret = pkgmgrinfo_pkginfo_get_metadata_value(
      pkginfo, "http://tizen.org/metadata/tizenclaw/llm-backend", &so_value);
  if (ret != PMINFO_R_OK || !so_value) {
    LOG(ERROR) << "Failed to find metadata llm-backend for " << pkgid;
    pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
    return false;
  }
  std::string so_local_path = so_value;
  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);

  // Usually plugins install their res/ to rootpath/res
  std::string config_path = path + "/res/plugin_llm_config.json";
  if (!std::filesystem::exists(config_path)) {
    // try fallback directly in path
    config_path = path + "/plugin_llm_config.json";
    if (!std::filesystem::exists(config_path)) {
      LOG(INFO) << "No plugin_llm_config.json found in " << pkgid
                << ", continuing with empty config";
    }
  }

  nlohmann::json config;
  if (std::filesystem::exists(config_path)) {
    std::ifstream f(config_path);
    try {
      f >> config;
    } catch (const std::exception& e) {
      LOG(ERROR) << "Failed to parse plugin_llm_config.json: " << e.what();
    }
  }

  std::string full_so_path = path + "/lib/" + so_local_path;

  auto backend =
      std::make_shared<PluginLlmBackend>(pkgid, full_so_path, config);
  if (!backend->Initialize()) {
    LOG(ERROR) << "Failed to initialize plugin backend from " << full_so_path;
    return false;
  }

  {
    std::lock_guard<std::mutex> lock(llm_backends_mutex_);
    llm_backends_.push_back(backend);
  }
  LOG(INFO) << "Successfully loaded " << backend->GetName() << " from "
            << pkgid;

  if (change_callback_) {
    change_callback_();
  }

  return true;
}

void PluginManager::UnloadPluginFromPkg(const std::string& pkgid) {
  std::vector<std::shared_ptr<PluginLlmBackend>> to_shutdown;
  {
    std::lock_guard<std::mutex> lock(llm_backends_mutex_);

    auto it =
        std::remove_if(llm_backends_.begin(), llm_backends_.end(),
                       [&pkgid](const std::shared_ptr<PluginLlmBackend>& b) {
                         return b->GetPkgId() == pkgid;
                       });
    if (it != llm_backends_.end()) {
      to_shutdown.insert(to_shutdown.end(), std::make_move_iterator(it),
                         std::make_move_iterator(llm_backends_.end()));
      llm_backends_.erase(it, llm_backends_.end());
    }
  }

  for (auto& backend : to_shutdown) {
    backend->Shutdown();
  }

  if (!to_shutdown.empty()) {
    LOG(INFO) << "Unloaded plugin(s) for pkg " << pkgid;
    if (change_callback_) {
      change_callback_();
    }
  }
}

bool PluginManager::LoadChannelPluginFromPkg(
    const std::string& pkgid) {
  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret = pkgmgrinfo_pkginfo_get_usr_pkginfo(
      pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) return false;

  char* so_value = nullptr;
  ret = pkgmgrinfo_pkginfo_get_metadata_value(
      pkginfo,
      "http://tizen.org/metadata/"
      "tizenclaw/channel",
      &so_value);
  if (ret != PMINFO_R_OK || !so_value) {
    pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
    return false;  // Not a channel plugin
  }
  std::string so_local = so_value;

  char* root_path = nullptr;
  ret = pkgmgrinfo_pkginfo_get_root_path(
      pkginfo, &root_path);
  if (ret != PMINFO_R_OK || !root_path) {
    LOG(ERROR) << "No root path for " << pkgid;
    pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
    return false;
  }
  std::string path(root_path);
  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);

  // Load optional config
  nlohmann::json config;
  std::string cfg_path =
      path + "/res/plugin_channel_config.json";
  if (!std::filesystem::exists(cfg_path))
    cfg_path = path + "/plugin_channel_config.json";
  if (std::filesystem::exists(cfg_path)) {
    std::ifstream f(cfg_path);
    try {
      f >> config;
    } catch (const std::exception& e) {
      LOG(ERROR) << "Bad channel plugin config: "
                 << e.what();
    }
  }

  std::string full_so =
      path + "/lib/" + so_local;

  auto ch = std::make_shared<PluginChannel>(
      pkgid, full_so, config, agent_);
  if (!ch->Initialize()) {
    LOG(ERROR) << "Channel plugin init failed: "
               << full_so;
    return false;
  }

  {
    std::lock_guard<std::mutex> lock(
        channel_plugins_mutex_);
    channel_plugins_.push_back(ch);
  }
  LOG(INFO) << "Channel plugin loaded: "
            << ch->GetName() << " from " << pkgid;

  if (change_callback_) change_callback_();
  return true;
}

void PluginManager::UnloadChannelPluginFromPkg(
    const std::string& pkgid) {
  std::vector<std::shared_ptr<PluginChannel>>
      to_stop;
  {
    std::lock_guard<std::mutex> lock(
        channel_plugins_mutex_);
    auto it = std::remove_if(
        channel_plugins_.begin(),
        channel_plugins_.end(),
        [&pkgid](
            const std::shared_ptr<PluginChannel>& c) {
          return c->GetPkgId() == pkgid;
        });
    if (it != channel_plugins_.end()) {
      to_stop.insert(
          to_stop.end(),
          std::make_move_iterator(it),
          std::make_move_iterator(
              channel_plugins_.end()));
      channel_plugins_.erase(
          it, channel_plugins_.end());
    }
  }

  for (auto& ch : to_stop) ch->Stop();

  if (!to_stop.empty()) {
    LOG(INFO) << "Unloaded channel plugin(s) for "
              << pkgid;
    if (change_callback_) change_callback_();
  }
}

}  // namespace tizenclaw

