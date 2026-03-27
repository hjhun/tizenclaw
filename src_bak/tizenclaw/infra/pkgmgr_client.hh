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

#ifndef PKGMGR_CLIENT_HH
#define PKGMGR_CLIENT_HH

#include <package-manager.h>

#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include "pkgmgr_event_args.hh"

namespace tizenclaw {

class PkgmgrClient {
 public:
  class IListener {
   public:
    virtual void OnPkgmgrEvent(std::shared_ptr<PkgmgrEventArgs> args) = 0;
  };

  PkgmgrClient();
  virtual ~PkgmgrClient();

  static PkgmgrClient& GetInstance();

  void AddListener(IListener* listener);
  void RemoveListener(IListener* listener);

 private:
  void StartListening();
  void StopListening();

  static int PkgmgrHandler(uid_t target_uid, int req_id, const char* pkg_type,
                           const char* pkgid, const char* key, const char* val,
                           const void* pmsg, void* user_data);

 private:
  std::mutex listeners_mutex_;
  std::vector<IListener*> listeners_;

  std::unique_ptr<pkgmgr_client, decltype(pkgmgr_client_free)*> handle_{
      nullptr, pkgmgr_client_free};
};

}  // namespace tizenclaw

#endif  // PKGMGR_CLIENT_HH
