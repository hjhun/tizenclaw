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

#include "bluetooth_controller.hh"

#include <bluetooth.h>
#include <system_info.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

std::string BluetoothController::GetInfo() const {
  bool supported = false;
  system_info_get_platform_bool(
      "http://tizen.org/feature/network.bluetooth",
      &supported);
  if (!supported)
    return "{\"error\": \"Bluetooth not supported\"}";

  if (bt_initialize() != 0)
    return "{\"error\": \"bt_initialize failed\"}";

  bt_adapter_state_e state;
  bt_adapter_get_state(&state);
  bool active = (state == BT_ADAPTER_ENABLED);

  std::string name;
  std::string addr;
  if (active) {
    char* n = nullptr;
    if (bt_adapter_get_name(&n) == 0 && n) {
      name = n;
      free(n);
    }
    char* a = nullptr;
    if (bt_adapter_get_address(&a) == 0 && a) {
      addr = a;
      free(a);
    }
  }

  bt_deinitialize();

  return "{\"activated\": " +
         std::string(active ? "true" : "false") +
         ", \"name\": \"" + name +
         "\", \"address\": \"" + addr + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
