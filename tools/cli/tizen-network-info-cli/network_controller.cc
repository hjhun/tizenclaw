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

#include "network_controller.hh"

#include <net_connection.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kTypeNames[] = {
    "disconnected", "wifi", "cellular",
    "ethernet", "bt", "net_proxy"};

}  // namespace

std::string NetworkController::GetNetworkInfo() const {
  connection_h conn = nullptr;
  if (connection_create(&conn) != 0)
    return "{\"error\": \"connection_create failed\"}";

  connection_type_e type;
  connection_get_type(conn, &type);
  const char* type_str =
      (type <= 5) ? kTypeNames[type] : "unknown";

  char* ip = nullptr;
  connection_get_ip_address(
      conn, CONNECTION_ADDRESS_FAMILY_IPV4, &ip);

  char* proxy = nullptr;
  connection_get_proxy(
      conn, CONNECTION_ADDRESS_FAMILY_IPV4, &proxy);

  std::string r =
      "{\"connection_type\": \"" +
      std::string(type_str) + "\", "
      "\"is_connected\": " +
      std::string(type != 0 ? "true" : "false") +
      ", \"ip_address\": \"" +
      (ip ? ip : "") + "\", "
      "\"proxy\": \"" + (proxy ? proxy : "") + "\"}";

  if (ip)
    free(ip);

  if (proxy)
    free(proxy);

  connection_destroy(conn);
  return r;
}

}  // namespace cli
}  // namespace tizenclaw
