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

#include "app_control_controller.hh"

#include <app_control.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string AppControlController::Launch(
    const std::string& app_id,
    const std::string& operation,
    const std::string& uri,
    const std::string& mime) {
  if (app_id.empty() && operation.empty()) {
    return "{\"error\": "
           "\"At least one of app_id or operation\"}";
  }

  app_control_h ac = nullptr;
  if (app_control_create(&ac) != 0)
    return "{\"error\": \"app_control_create failed\"}";

  if (!operation.empty()) {
    app_control_set_operation(ac, operation.c_str());
  } else if (!app_id.empty()) {
    app_control_set_operation(
        ac, APP_CONTROL_OPERATION_DEFAULT);
  }

  if (!app_id.empty())
    app_control_set_app_id(ac, app_id.c_str());

  if (!uri.empty())
    app_control_set_uri(ac, uri.c_str());

  if (!mime.empty())
    app_control_set_mime(ac, mime.c_str());

  int ret = app_control_send_launch_request(
      ac, nullptr, nullptr);
  app_control_destroy(ac);

  if (ret != 0) {
    return "{\"error\": \"Launch failed (code: " +
           std::to_string(ret) + ")\"}";
  }

  std::string r = "{\"result\": \"launched\"";
  if (!app_id.empty())
    r += ", \"app_id\": \"" + app_id + "\"";

  if (!operation.empty())
    r += ", \"operation\": \"" + operation + "\"";

  r += "}";
  return r;
}

}  // namespace cli
}  // namespace tizenclaw
