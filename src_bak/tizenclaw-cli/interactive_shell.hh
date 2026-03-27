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

#ifndef TIZENCLAW_CLI_INTERACTIVE_SHELL_HH_
#define TIZENCLAW_CLI_INTERACTIVE_SHELL_HH_

#include <string>

#include "request_handler.hh"

namespace tizenclaw {
namespace cli {

class InteractiveShell {
 public:
  explicit InteractiveShell(
      RequestHandler& handler);

  // Run the interactive REPL loop.
  void Run(const std::string& session_id,
           bool stream);

 private:
  RequestHandler& handler_;
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_INTERACTIVE_SHELL_HH_
