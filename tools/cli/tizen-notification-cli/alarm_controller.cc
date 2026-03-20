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

#include "alarm_controller.hh"

#include <app_alarm.h>
#include <app_control.h>

#include <cstring>
#include <ctime>
#include <string>

namespace tizenclaw {
namespace cli {

std::string AlarmController::Schedule(
    const std::string& app_id,
    const std::string& datetime) {
  struct tm t;
  memset(&t, 0, sizeof(t));

  if (!strptime(datetime.c_str(),
                "%Y-%m-%dT%H:%M:%S", &t))
    return "{\"error\": \"Invalid datetime format\"}";

  t.tm_isdst = -1;

  app_control_h ac = nullptr;
  if (app_control_create(&ac) != 0)
    return "{\"error\": \"app_control_create failed\"}";

  app_control_set_app_id(ac, app_id.c_str());

  int alarm_id = 0;
  int ret = alarm_schedule_once_at_date(
      ac, &t, &alarm_id);
  app_control_destroy(ac);

  if (ret != 0) {
    return "{\"error\": \"alarm_schedule failed "
           "(code: " + std::to_string(ret) + ")\"}";
  }

  return "{\"status\": \"success\", "
         "\"alarm_id\": " +
         std::to_string(alarm_id) +
         ", \"app_id\": \"" + app_id + "\", "
         "\"scheduled_time\": \"" + datetime + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
