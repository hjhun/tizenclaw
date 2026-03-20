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

#include "wifi_controller.hh"

#include <wifi-manager.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kStateNames[] = {
    "disconnected", "association", "configuration",
    "connected", "failure"};

}  // namespace

std::string WifiController::GetWifiInfo() const {
  wifi_manager_h mgr = nullptr;
  if (wifi_manager_initialize(&mgr) != 0)
    return "{\"error\": \"wifi_manager_initialize\"}";

  bool activated = false;
  wifi_manager_is_activated(mgr, &activated);

  std::string essid;
  std::string cs = "unknown";

  if (activated) {
    wifi_manager_connection_state_e state;
    wifi_manager_get_connection_state(mgr, &state);
    cs = (state <= 4) ? kStateNames[state] : "unknown";

    if (state ==
        WIFI_MANAGER_CONNECTION_STATE_CONNECTED) {
      wifi_manager_ap_h ap = nullptr;
      if (wifi_manager_get_connected_ap(
              mgr, &ap) == 0) {
        char* e = nullptr;
        if (wifi_manager_ap_get_essid(ap, &e) == 0 &&
            e) {
          essid = e;
          free(e);
        }

        wifi_manager_ap_destroy(ap);
      }
    }
  }

  wifi_manager_deinitialize(mgr);

  return "{\"activated\": " +
         std::string(activated ? "true" : "false") +
         ", \"connection_state\": \"" + cs +
         "\", \"essid\": \"" + essid + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
