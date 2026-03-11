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

#ifndef TIZENCLAW_INFRA_PKGMGR_EVENT_ARGS_HH_
#define TIZENCLAW_INFRA_PKGMGR_EVENT_ARGS_HH_

#include <sys/types.h>
#include <string>

namespace tizenclaw {

class PkgmgrEventArgs {
 public:
  PkgmgrEventArgs(uid_t target_uid, int req_id, std::string pkg_type,
                  std::string pkgid, std::string event_status,
                  std::string event_name);

  uid_t GetTargetUid() const;
  int GetReqId() const;
  const std::string& GetPkgType() const;
  const std::string& GetPkgId() const;
  const std::string& GetEventStatus() const;
  const std::string& GetEventName() const;
  const std::string& GetTag() const;

 private:
  uid_t target_uid_;
  int req_id_;
  std::string pkg_type_;
  std::string pkgid_;
  std::string event_status_;
  std::string event_name_;
  std::string tag_;
};

}  // namespace tizenclaw

#endif  // TIZENCLAW_INFRA_PKGMGR_EVENT_ARGS_HH_
