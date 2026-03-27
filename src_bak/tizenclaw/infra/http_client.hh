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
#ifndef HTTP_CLIENT_HH
#define HTTP_CLIENT_HH

#include <functional>
#include <map>
#include <string>

namespace tizenclaw {

struct HttpResponse {
  long status_code = 0;
  std::string body;
  bool success = false;
  std::string error;
};

class HttpClient {
 public:
  // POST JSON with retry + exponential backoff.
  [[nodiscard]] static HttpResponse Post(
      const std::string& url, const std::map<std::string, std::string>& headers,
      const std::string& json_body, int max_retries = 3,
      long connect_timeout_sec = 10, long request_timeout_sec = 30,
      std::function<void(const std::string&)> stream_cb = nullptr);

  // GET with retry + timeouts (for long polling)
  [[nodiscard]] static HttpResponse Get(
      const std::string& url,
      const std::map<std::string, std::string>& headers = {},
      int max_retries = 3, long connect_timeout_sec = 10,
      long request_timeout_sec = 40);
};

}  // namespace tizenclaw

#endif  // HTTP_CLIENT_HH
