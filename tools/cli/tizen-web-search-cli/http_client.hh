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

#ifndef TIZENCLAW_CLI_HTTP_CLIENT_HH_
#define TIZENCLAW_CLI_HTTP_CLIENT_HH_

#include <string>

namespace tizenclaw {
namespace cli {

class HttpClient {
 public:
  HttpClient() = default;
  ~HttpClient() = default;

  struct Response {
    int status_code = 0;
    std::string body;
    std::string error;
  };

  Response Get(
      const std::string& url,
      const std::string& extra_headers = "") const;

  Response Post(
      const std::string& url,
      const std::string& json_body,
      const std::string& extra_headers = "") const;
};

}  // namespace cli
}  // namespace tizenclaw

#endif  // TIZENCLAW_CLI_HTTP_CLIENT_HH_
