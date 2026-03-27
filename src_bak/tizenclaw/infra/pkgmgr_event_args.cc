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

#include "pkgmgr_event_args.hh"

#include <utility>

namespace tizenclaw {

PkgmgrEventArgs::PkgmgrEventArgs(uid_t target_uid, int req_id,
                                 std::string pkg_type, std::string pkgid,
                                 std::string event_status,
                                 std::string event_name)
    : target_uid_(target_uid),
      req_id_(req_id),
      pkg_type_(std::move(pkg_type)),
      pkgid_(std::move(pkgid)),
      event_status_(std::move(event_status)),
      event_name_(std::move(event_name)) {
  tag_ = std::to_string(target_uid) + "-" + pkgid_;
}

uid_t PkgmgrEventArgs::GetTargetUid() const { return target_uid_; }
int PkgmgrEventArgs::GetReqId() const { return req_id_; }
const std::string& PkgmgrEventArgs::GetPkgType() const { return pkg_type_; }
const std::string& PkgmgrEventArgs::GetPkgId() const { return pkgid_; }
const std::string& PkgmgrEventArgs::GetEventStatus() const {
  return event_status_;
}
const std::string& PkgmgrEventArgs::GetEventName() const { return event_name_; }
const std::string& PkgmgrEventArgs::GetTag() const { return tag_; }

}  // namespace tizenclaw
