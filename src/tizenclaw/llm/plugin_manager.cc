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
  char* res_path = nullptr;
  // Fallback to pkgmgrinfo_pkginfo_get_res_path or
  // pkgmgrinfo_appinfo_get_res_path. In Tizen, there is
  // pkgmgrinfo_pkginfo_get_pkginfo_string() available for generic properties.
  // Actually, there is an explicit API: pkgmgrinfo_pkginfo_get_res_path in
  // some profiles. Wait, let's try the common one.
  pkgmgrinfo_pkginfo_h pkginfo = nullptr;
  int ret = pkgmgrinfo_pkginfo_get_usr_pkginfo(
      pkgid.c_str(), getuid(), &pkginfo);
  if (ret != PMINFO_R_OK || !pkginfo) {
    LOG(ERROR) << "Failed to get pkginfo for " << pkgid;
    return false;
  }

  ret = pkgmgrinfo_pkginfo_get_root_path(pkginfo, &res_path);
  pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);

  if (ret != PMINFO_R_OK || !res_path) {
    LOG(ERROR) << "Failed to get root path for " << pkgid;
    return false;
  }
  
  std::string path(res_path);
  free(res_path);

  // Usually plugins install their res/ to rootpath/res
  std::string config_path = path + "/res/plugin_llm_config.json";
  if (!std::filesystem::exists(config_path)) {
    // try fallback directly in path
    config_path = path + "/plugin_llm_config.json";
    if (!std::filesystem::exists(config_path)) {
      LOG(INFO) << "No plugin_llm_config.json found in " << pkgid
                << ", skipping";
      return false;
    }
  }

  std::ifstream f(config_path);
  nlohmann::json config;
  try {
    f >> config;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse plugin_llm_config.json: " << e.what();
    return false;
  }

  if (!config.contains("path")) {
    LOG(ERROR) << "Config missing path";
    return false;
  }

  std::string so_local_path = config["path"].get<std::string>();
  std::string full_so_path = path + "/" + so_local_path;

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
  std::lock_guard<std::mutex> lock(llm_backends_mutex_);
  
  auto it = std::remove_if(
      llm_backends_.begin(), llm_backends_.end(),
      [&pkgid](const std::shared_ptr<PluginLlmBackend>& b) {
                             return b->GetPkgId() == pkgid;
                           });
  if (it != llm_backends_.end()) {
    for (auto i = it; i != llm_backends_.end(); ++i) {
      (*i)->Shutdown();
    }
    llm_backends_.erase(it, llm_backends_.end());
    LOG(INFO) << "Unloaded plugin(s) for pkg " << pkgid;
  }
}

} // namespace tizenclaw
