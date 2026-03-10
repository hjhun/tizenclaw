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
#include "plugin_llm_backend.hh"
#include "../../common/logging.hh"

#include <pkgmgr-info.h>
#include <fstream>
#include <thread>

namespace tizenclaw {

PluginManager& PluginManager::GetInstance() {
  static PluginManager instance;
  return instance;
}

PluginManager::PluginManager() {}

PluginManager::~PluginManager() {
  Shutdown();
}

bool PluginManager::Initialize() {
  StartListening();

  pkgmgrinfo_pkginfo_metadata_filter_h filter;
  int ret = pkgmgrinfo_pkginfo_metadata_filter_create(&filter);
  if (ret != PMINFO_R_OK) {
    LOG(ERROR) << "Failed to create metadata filter: " << ret;
    return true; // Graceful fallback for headless unit tests
  }

  pkgmgrinfo_pkginfo_metadata_filter_add(
      filter, "http://tizen.org/metadata/tizenclaw/llm-backend", nullptr);

  pkgmgrinfo_pkginfo_metadata_filter_foreach(
      filter,
      [](pkgmgrinfo_pkginfo_h handle, void* user_data) {
        auto* manager = static_cast<PluginManager*>(user_data);
        char* pkgid = nullptr;
        if (pkgmgrinfo_pkginfo_get_pkgid(handle, &pkgid) == PMINFO_R_OK && pkgid) {
          manager->LoadPluginFromPkg(pkgid);
        }
        return 0;
      },
      this);

  pkgmgrinfo_pkginfo_metadata_filter_destroy(filter);
  return true;
}

void PluginManager::Shutdown() {
  StopListening();
  std::lock_guard<std::mutex> lock(llm_backends_mutex_);
  for (auto& backend : llm_backends_) {
    backend->Shutdown();
  }
  llm_backends_.clear();
}

std::vector<std::shared_ptr<PluginLlmBackend>>
PluginManager::GetLlmBackends() {
  std::lock_guard<std::mutex> lock(llm_backends_mutex_);
  return llm_backends_;
}

void PluginManager::StartListening() {
  if (pkgmgr_handle_) return;

  auto* handle = pkgmgr_client_new(PC_LISTENING);
  if (!handle) {
    LOG(ERROR) << "Failed to create pkgmgr_client";
    return;
  }
  
  pkgmgr_handle_ =
      std::unique_ptr<pkgmgr_client, decltype(pkgmgr_client_free)*>(
          handle, pkgmgr_client_free);
  
  int ret = pkgmgr_client_set_status_type(handle, PKGMGR_CLIENT_STATUS_ALL);
  if (ret < 0) {
    LOG(ERROR) << "Failed to set pkgmgr_client status type: " << ret;
  }

  ret = pkgmgr_client_listen_status(handle, PkgmgrHandler, this);
  if (ret < 0) {
    LOG(ERROR) << "Failed to listen pkgmgr status: " << ret;
  }
}

void PluginManager::StopListening() {
  pkgmgr_handle_.reset();
}

int PluginManager::PkgmgrHandler(uid_t target_uid, int req_id,
                                 const char* pkg_type, const char* pkgid,
                                 const char* key, const char* val,
                                 const void* pmsg, void* user_data) {
  if (!pkgid || !key || !val) return 0;
  
  PluginManager* self = static_cast<PluginManager*>(user_data);
  std::string s_key = key;
  std::string s_val = val;
  std::string s_pkgid = pkgid;

  // We only care about successful completion of install/upgrade/uninstall
  if (s_val != "ok") return 0;

  if (s_key == "install" || s_key == "recoverinstall") {
    self->HandleInstallEvent(s_pkgid);
  } else if (s_key == "upgrade" || s_key == "recoverupgrade") {
    self->HandleUpdateEvent(s_pkgid);
  } else if (s_key == "uninstall" || s_key == "recoveruninstall") {
    self->HandleUninstallEvent(s_pkgid);
  }

  return 0;
}

void PluginManager::HandleInstallEvent(const std::string& pkgid) {
  LOG(INFO) << "Install event for " << pkgid;
  LoadPluginFromPkg(pkgid);
}

void PluginManager::HandleUpdateEvent(const std::string& pkgid) {
  LOG(INFO) << "Update event for " << pkgid;
  UnloadPluginFromPkg(pkgid);
  LoadPluginFromPkg(pkgid);
}

void PluginManager::HandleUninstallEvent(const std::string& pkgid) {
  LOG(INFO) << "Uninstall event for " << pkgid;
  UnloadPluginFromPkg(pkgid);
}

bool PluginManager::LoadPluginFromPkg(const std::string& pkgid) {
  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret = pkgmgrinfo_pkginfo_get_usr_pkginfo(
      pkgid.c_str(), getuid(), &pkginfo);
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

  std::lock_guard<std::mutex> lock(llm_backends_mutex_);
  llm_backends_.push_back(backend);
  LOG(INFO) << "Successfully loaded " << backend->GetName() << " from "
            << pkgid;

  return true;
}

void PluginManager::UnloadPluginFromPkg(const std::string& pkgid) {
  std::vector<std::shared_ptr<PluginLlmBackend>> to_shutdown;
  {
    std::lock_guard<std::mutex> lock(llm_backends_mutex_);
    
    auto it = std::remove_if(
        llm_backends_.begin(), llm_backends_.end(),
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
  }
}

} // namespace tizenclaw
