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

#include "notification_controller.hh"

#include <notification.h>

#include <string>

namespace tizenclaw {
namespace cli {

std::string NotificationController::Send(
    const std::string& title,
    const std::string& body) {
  notification_h noti =
      notification_create(NOTIFICATION_TYPE_NOTI);
  if (!noti)
    return "{\"error\": \"Failed to create notification\"}";

  int ret = notification_set_text(
      noti, NOTIFICATION_TEXT_TYPE_TITLE,
      title.c_str(), nullptr,
      NOTIFICATION_VARIABLE_TYPE_NONE);
  if (ret != 0) {
    notification_free(noti);
    return "{\"error\": \"Failed to set title\"}";
  }

  ret = notification_set_text(
      noti, NOTIFICATION_TEXT_TYPE_CONTENT,
      body.c_str(), nullptr,
      NOTIFICATION_VARIABLE_TYPE_NONE);
  if (ret != 0) {
    notification_free(noti);
    return "{\"error\": \"Failed to set content\"}";
  }

  ret = notification_post(noti);
  notification_free(noti);

  if (ret != 0) {
    return "{\"error\": \"Failed to post (code: " +
           std::to_string(ret) + ")\"}";
  }

  return "{\"status\": \"success\", "
         "\"title\": \"" + title + "\", "
         "\"body\": \"" + body + "\"}";
}

}  // namespace cli
}  // namespace tizenclaw
