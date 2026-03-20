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

#include "http_client.hh"

#include <curl/curl.h>

#include <cstring>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

size_t WriteCallback(void* contents, size_t size,
                     size_t nmemb, void* userp) {
  auto* str = static_cast<std::string*>(userp);
  size_t total = size * nmemb;
  str->append(static_cast<char*>(contents), total);
  return total;
}

struct CurlSlistGuard {
  curl_slist* list = nullptr;
  ~CurlSlistGuard() {
    if (list)
      curl_slist_free_all(list);
  }
};

}  // namespace

HttpClient::Response HttpClient::Get(
    const std::string& url,
    const std::string& extra_headers) const {
  Response resp;
  CURL* curl = curl_easy_init();
  if (!curl) {
    resp.error = "curl_easy_init failed";
    return resp;
  }

  curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION,
                   WriteCallback);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA,
                   &resp.body);
  curl_easy_setopt(curl, CURLOPT_TIMEOUT, 15L);
  curl_easy_setopt(curl, CURLOPT_USERAGENT,
                   "TizenClaw/3.0");

  CurlSlistGuard slist_guard;
  if (!extra_headers.empty()) {
    // Parse newline-separated headers
    std::string h = extra_headers;
    size_t pos = 0;
    while ((pos = h.find('\n')) != std::string::npos) {
      std::string line = h.substr(0, pos);
      if (!line.empty()) {
        slist_guard.list = curl_slist_append(
            slist_guard.list, line.c_str());
      }
      h = h.substr(pos + 1);
    }

    if (!h.empty()) {
      slist_guard.list = curl_slist_append(
          slist_guard.list, h.c_str());
    }

    if (slist_guard.list) {
      curl_easy_setopt(curl, CURLOPT_HTTPHEADER,
                       slist_guard.list);
    }
  }

  CURLcode res = curl_easy_perform(curl);
  if (res != CURLE_OK) {
    resp.error = curl_easy_strerror(res);
  } else {
    long code = 0;
    curl_easy_getinfo(curl,
                      CURLINFO_RESPONSE_CODE,
                      &code);
    resp.status_code = static_cast<int>(code);
  }

  curl_easy_cleanup(curl);
  return resp;
}

HttpClient::Response HttpClient::Post(
    const std::string& url,
    const std::string& json_body,
    const std::string& extra_headers) const {
  Response resp;
  CURL* curl = curl_easy_init();
  if (!curl) {
    resp.error = "curl_easy_init failed";
    return resp;
  }

  curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
  curl_easy_setopt(curl, CURLOPT_POST, 1L);
  curl_easy_setopt(curl, CURLOPT_POSTFIELDS,
                   json_body.c_str());
  curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE,
                   static_cast<long>(
                       json_body.size()));
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION,
                   WriteCallback);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA,
                   &resp.body);
  curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
  curl_easy_setopt(curl, CURLOPT_USERAGENT,
                   "TizenClaw/3.0");

  CurlSlistGuard slist_guard;
  slist_guard.list = curl_slist_append(
      nullptr, "Content-Type: application/json");

  if (!extra_headers.empty()) {
    std::string h = extra_headers;
    size_t pos = 0;
    while ((pos = h.find('\n')) != std::string::npos) {
      std::string line = h.substr(0, pos);
      if (!line.empty()) {
        slist_guard.list = curl_slist_append(
            slist_guard.list, line.c_str());
      }
      h = h.substr(pos + 1);
    }

    if (!h.empty()) {
      slist_guard.list = curl_slist_append(
          slist_guard.list, h.c_str());
    }
  }

  curl_easy_setopt(curl, CURLOPT_HTTPHEADER,
                   slist_guard.list);

  CURLcode res = curl_easy_perform(curl);
  if (res != CURLE_OK) {
    resp.error = curl_easy_strerror(res);
  } else {
    long code = 0;
    curl_easy_getinfo(curl,
                      CURLINFO_RESPONSE_CODE,
                      &code);
    resp.status_code = static_cast<int>(code);
  }

  curl_easy_cleanup(curl);
  return resp;
}

}  // namespace cli
}  // namespace tizenclaw
