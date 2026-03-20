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

#include "data_usage_controller.hh"

#include <net_connection.h>

#include <cstdio>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

using StatType = connection_statistics_type_e;

long long GetStat(connection_h conn,
                  connection_type_e type,
                  StatType stat) {
  long long val = 0;
  connection_get_statistics(conn, type, stat, &val);
  return val;
}

}  // namespace

std::string DataUsageController::GetDataUsage() const {
  connection_h conn = nullptr;
  if (connection_create(&conn) != 0)
    return "{\"error\": \"connection_create failed\"}";

  long long wlr = GetStat(
      conn, CONNECTION_TYPE_WIFI,
      CONNECTION_STATISTICS_TYPE_LAST_RECEIVED_DATA);
  long long wls = GetStat(
      conn, CONNECTION_TYPE_WIFI,
      CONNECTION_STATISTICS_TYPE_LAST_SENT_DATA);
  long long wtr = GetStat(
      conn, CONNECTION_TYPE_WIFI,
      CONNECTION_STATISTICS_TYPE_TOTAL_RECEIVED_DATA);
  long long wts = GetStat(
      conn, CONNECTION_TYPE_WIFI,
      CONNECTION_STATISTICS_TYPE_TOTAL_SENT_DATA);

  long long clr = GetStat(
      conn, CONNECTION_TYPE_CELLULAR,
      CONNECTION_STATISTICS_TYPE_LAST_RECEIVED_DATA);
  long long cls = GetStat(
      conn, CONNECTION_TYPE_CELLULAR,
      CONNECTION_STATISTICS_TYPE_LAST_SENT_DATA);
  long long ctr = GetStat(
      conn, CONNECTION_TYPE_CELLULAR,
      CONNECTION_STATISTICS_TYPE_TOTAL_RECEIVED_DATA);
  long long cts = GetStat(
      conn, CONNECTION_TYPE_CELLULAR,
      CONNECTION_STATISTICS_TYPE_TOTAL_SENT_DATA);

  connection_destroy(conn);

  constexpr double kBytesPerMb = 1048576.0;
  char wifi_mb[16];
  char cell_mb[16];
  snprintf(wifi_mb, sizeof(wifi_mb), "%.2f",
           (wtr + wts) / kBytesPerMb);
  snprintf(cell_mb, sizeof(cell_mb), "%.2f",
           (ctr + cts) / kBytesPerMb);

  return "{\"data_usage\": {"
         "\"wifi\": {"
         "\"last_received_bytes\": " +
         std::to_string(wlr) +
         ", \"last_sent_bytes\": " +
         std::to_string(wls) +
         ", \"total_received_bytes\": " +
         std::to_string(wtr) +
         ", \"total_sent_bytes\": " +
         std::to_string(wts) +
         ", \"total_mb\": " + wifi_mb + "}, "
         "\"cellular\": {"
         "\"last_received_bytes\": " +
         std::to_string(clr) +
         ", \"last_sent_bytes\": " +
         std::to_string(cls) +
         ", \"total_received_bytes\": " +
         std::to_string(ctr) +
         ", \"total_sent_bytes\": " +
         std::to_string(cts) +
         ", \"total_mb\": " + cell_mb + "}}}";
}

}  // namespace cli
}  // namespace tizenclaw
