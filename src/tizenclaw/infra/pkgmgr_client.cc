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

#include "pkgmgr_client.hh"
#include "../../common/logging.hh"

#include <algorithm>

namespace tizenclaw {

PkgmgrClient& PkgmgrClient::GetInstance() {
  static PkgmgrClient instance;
  return instance;
}

PkgmgrClient::PkgmgrClient() {}

PkgmgrClient::~PkgmgrClient() { StopListening(); }

void PkgmgrClient::AddListener(IListener* listener) {
  std::lock_guard<std::mutex> lock(listeners_mutex_);
  if (std::find(listeners_.begin(), listeners_.end(), listener) ==
      listeners_.end()) {
    listeners_.push_back(listener);
    
    // First listener being added, start the actual pkgmgr client
    if (listeners_.size() == 1) {
      StartListening();
    }
  }
}

void PkgmgrClient::RemoveListener(IListener* listener) {
  std::lock_guard<std::mutex> lock(listeners_mutex_);
  auto it = std::find(listeners_.begin(), listeners_.end(), listener);
  if (it != listeners_.end()) {
    listeners_.erase(it);
    
    // Last listener removed, we can stop the client
    if (listeners_.empty()) {
      StopListening();
    }
  }
}

void PkgmgrClient::StartListening() {
  if (handle_) return;

  auto* handle = pkgmgr_client_new(PC_LISTENING);
  if (!handle) {
    LOG(ERROR) << "pkgmgr_client_new() is failed";
    return;
  }

  handle_ = std::unique_ptr<pkgmgr_client, decltype(pkgmgr_client_free)*>(
      handle, pkgmgr_client_free);

  int ret = pkgmgr_client_set_status_type(handle, PKGMGR_CLIENT_STATUS_ALL);
  if (ret < 0) {
    LOG(ERROR) << "pkgmgr_client_set_status_type() is failed. error=" << ret;
  }

  ret = pkgmgr_client_listen_status(handle, PkgmgrHandler, this);
  if (ret < 0) {
    LOG(ERROR) << "pkgmgr_client_listen_status() is failed. error=" << ret;
  }
}

void PkgmgrClient::StopListening() {
  if (handle_) {
    handle_.reset();
  }
}

int PkgmgrClient::PkgmgrHandler(uid_t target_uid, int req_id,
                                const char* pkg_type, const char* pkgid,
                                const char* key, const char* val,
                                const void* pmsg, void* user_data) {
  if (!pkg_type || !pkgid || !key || !val) return 0;

  auto* self = static_cast<PkgmgrClient*>(user_data);
  
  std::string s_pkg_type = pkg_type;
  std::string s_pkgid = pkgid;
  std::string s_event_status = key; // key is the status (start, end, error)
  std::string s_event_name = val;   // val is the event (install, upgrade, uninstall, etc.)

  std::lock_guard<std::mutex> lock(self->listeners_mutex_);
  for (auto* listener : self->listeners_) {
    listener->OnPkgmgrEvent(std::make_shared<PkgmgrEventArgs>(
        target_uid, req_id, s_pkg_type, s_pkgid, s_event_status, s_event_name));
  }

  return 0;
}

}  // namespace tizenclaw
