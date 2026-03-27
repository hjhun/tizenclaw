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
#ifndef PACKAGE_EVENT_ADAPTER_HH
#define PACKAGE_EVENT_ADAPTER_HH

#include <package-manager.h>

#include <json.hpp>
#include <string>

#include "event_adapter.hh"

namespace tizenclaw {

// Monitors package install/uninstall/update events
// using the lower-level pkgmgr_client API directly
// to avoid cynara privilege checks that fail inside
// the container environment.
class PackageEventAdapter : public IEventAdapter {
 public:
  PackageEventAdapter() = default;
  ~PackageEventAdapter() override;

  void Start() override;
  void Stop() override;
  [[nodiscard]] std::string GetName() const override;

 private:
  // Callback from pkgmgr_client_listen_status
  static int OnPackageEvent(
      uid_t target_uid, int req_id,
      const char* pkg_type,
      const char* pkg_name,
      const char* key, const char* val,
      const void* pmsg, void* user_data);

  // Query app info for a given package
  static nlohmann::json QueryAppInfo(
      const char* package_id);

  pkgmgr_client* client_ = nullptr;
  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // PACKAGE_EVENT_ADAPTER_HH
