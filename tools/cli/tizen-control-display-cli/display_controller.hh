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

#ifndef TIZENCLAW_CLI_DISPLAY_CONTROLLER_HH_
#define TIZENCLAW_CLI_DISPLAY_CONTROLLER_HH_

#include <string>

namespace tizenclaw {
namespace cli {

class DisplayController {
 public:
  DisplayController() = default;
  ~DisplayController() = default;

  /**
   * @brief Sets the display brightness
   * @param brightness The brightness value to set
   * @return JSON string containing the result
   */
  std::string SetBrightness(int brightness);

  /**
   * @brief Gets current and max display brightness
   * @return JSON string containing the info
   */
  std::string GetInfo();

 private:
  std::string CreateErrorJson(const std::string& error_msg);
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_DISPLAY_CONTROLLER_HH_
