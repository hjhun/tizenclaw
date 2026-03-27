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

#ifndef TIZENCLAW_CLI_RESPONSE_PRINTER_HH_
#define TIZENCLAW_CLI_RESPONSE_PRINTER_HH_

#include <string>

namespace tizenclaw {
namespace cli {

class ResponsePrinter {
 public:
  // Pretty-print the list_agents JSON response.
  static void PrintAgentList(
      const std::string& body);

  // Pretty-print the perception status JSON
  // response.
  static void PrintPerceptionStatus(
      const std::string& body);

  // Pretty-print the list_system_cli JSON response.
  static void PrintToolList(
      const std::string& body);

  // Pretty-print the list_mcp_tools JSON response.
  static void PrintMcpToolList(
      const std::string& body);

  // Pretty-print register/unregister result.
  static void PrintToolResult(
      const std::string& body);
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_RESPONSE_PRINTER_HH_
