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

#ifndef TIZENCLAW_CLI_REQUEST_HANDLER_HH_
#define TIZENCLAW_CLI_REQUEST_HANDLER_HH_

#include <string>

#include "tizenclaw.h"

namespace tizenclaw {
namespace cli {

class RequestHandler {
 public:
  RequestHandler();
  ~RequestHandler();

  // Initialize the CAPI client.
  // Returns true on success.
  [[nodiscard]] bool Create();

  // Send a prompt and block until response.
  // Returns the response string (empty on error).
  [[nodiscard]] std::string SendRequest(
      const std::string& session_id,
      const std::string& prompt,
      bool stream);

 private:
  tizenclaw_client_h client_;
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_REQUEST_HANDLER_HH_
