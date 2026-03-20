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

#include "terminate_controller.hh"

#include <app_manager.h>
#include <app_manager_extension.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string TerminateController::Terminate(
    const std::string& app_id) {
  bool running = false;
  int ret = app_manager_is_running(
      app_id.c_str(), &running);
  if (ret != 0)
    return "{\"error\": \"app_manager_is_running failed\"}";

  if (!running) {
    return "{\"status\": \"not_running\", "
           "\"app_id\": \"" + app_id + "\"}";
  }

  app_context_h ctx = nullptr;
  ret = app_manager_get_app_context(
      app_id.c_str(), &ctx);
  if (ret != 0)
    return "{\"error\": \"Failed to get app context\"}";

  ret = app_manager_terminate_app(ctx);
  app_context_destroy(ctx);

  if (ret != 0) {
    return "{\"error\": \"Failed to terminate (code: " +
           std::to_string(ret) + ")\"}";
  }

  return "{\"status\": \"success\", "
         "\"app_id\": \"" + app_id + "\", "
         "\"message\": \"App terminated\"}";
}

}  // namespace cli
}  // namespace tizenclaw
