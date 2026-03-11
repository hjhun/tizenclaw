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

#include <chrono>
#include <thread>

#include "../../libtizenclaw-core/inc/tizenclaw_llm_backend.h"
#include "../../libtizenclaw-core/inc/tizenclaw_curl.h"
#include "../../common/logging.hh"

namespace tizenclaw {

struct WriteContext {
  std::string* body;
  std::function<void(const std::string&)> stream_cb;
};

static void LlmWrapperChunkCb(const char* chunk, void* user_data) {
  WriteContext* ctx = static_cast<WriteContext*>(user_data);
  if (chunk && ctx->body) {
    ctx->body->append(chunk);
  }
  if (chunk && ctx->stream_cb) {
    ctx->stream_cb(chunk);
  }
}

HttpResponse HttpClient::Post(
    const std::string& url, const std::map<std::string, std::string>& hdrs,
    const std::string& json_body, int max_retries, long connect_timeout_sec,
    long request_timeout_sec,
    std::function<void(const std::string&)> stream_cb) {
  HttpResponse result;

  for (int attempt = 0; attempt < max_retries; ++attempt) {
    if (attempt > 0) {
      int delay_ms = 1000 * (1 << (attempt - 1));
      LOG(WARNING) << "Retry " << attempt << " after " << delay_ms << "ms";
      std::this_thread::sleep_for(std::chrono::milliseconds(delay_ms));
    }

    result.body.clear();
    result.error.clear();

    tizenclaw_curl_h curl = nullptr;
    if (tizenclaw_curl_create(&curl) != TIZENCLAW_ERROR_NONE) {
      result.error = "tizenclaw_curl_create() failed";
      continue;
    }

    tizenclaw_curl_set_url(curl, url.c_str());

    for (auto& [k, v] : hdrs) {
      std::string h = k + ": " + v;
      tizenclaw_curl_add_header(curl, h.c_str());
    }

    tizenclaw_curl_set_post_data(curl, json_body.c_str());

    WriteContext write_ctx;
    write_ctx.body = &result.body;
    write_ctx.stream_cb = stream_cb;

    tizenclaw_curl_set_write_callback(curl, LlmWrapperChunkCb, &write_ctx);
    tizenclaw_curl_set_timeout(curl, connect_timeout_sec, request_timeout_sec);

    int res = tizenclaw_curl_perform(curl);
    
    long scode = 0;
    tizenclaw_curl_get_response_code(curl, &scode);
    result.status_code = static_cast<int>(scode);

    if (res != TIZENCLAW_ERROR_NONE) {
      const char* err = tizenclaw_curl_get_error_message(curl);
      result.error = err ? err : "Unknown error";
      LOG(ERROR) << "curl failed: " << result.error << " (" << (attempt + 1)
                 << "/" << max_retries << ")";
      tizenclaw_curl_destroy(curl);
      continue;
    }
    
    tizenclaw_curl_destroy(curl);

    if (result.status_code == 429 || result.status_code >= 500) {
      result.error =
          "HTTP " + std::to_string(result.status_code) + " (Retry limit)";
      LOG(WARNING) << "HTTP " << result.status_code << ", retry ("
                   << (attempt + 1) << "/" << max_retries << ")";
      continue;
    }

    result.success = (result.status_code >= 200 && result.status_code < 300);
    if (!result.success) {
      result.error = "HTTP " + std::to_string(result.status_code);
    }
    return result;
  }

  LOG(ERROR) << "All " << max_retries << " retries failed";
  result.success = false;
  return result;
}

HttpResponse HttpClient::Get(const std::string& url,
                             const std::map<std::string, std::string>& hdrs,
                             int max_retries, long connect_timeout_sec,
                             long request_timeout_sec) {
  HttpResponse result;

  for (int attempt = 0; attempt < max_retries; ++attempt) {
    if (attempt > 0) {
      int delay_ms = 1000 * (1 << (attempt - 1));
      LOG(WARNING) << "Retry " << attempt << " after " << delay_ms << "ms";
      std::this_thread::sleep_for(std::chrono::milliseconds(delay_ms));
    }

    result.body.clear();
    result.error.clear();

    tizenclaw_curl_h curl = nullptr;
    if (tizenclaw_curl_create(&curl) != TIZENCLAW_ERROR_NONE) {
      result.error = "tizenclaw_curl_create() failed";
      continue;
    }

    tizenclaw_curl_set_url(curl, url.c_str());

    for (auto& [k, v] : hdrs) {
      std::string h = k + ": " + v;
      tizenclaw_curl_add_header(curl, h.c_str());
    }

    tizenclaw_curl_set_method_get(curl);

    WriteContext write_ctx;
    write_ctx.body = &result.body;
    write_ctx.stream_cb = nullptr;

    tizenclaw_curl_set_write_callback(curl, LlmWrapperChunkCb, &write_ctx);
    tizenclaw_curl_set_timeout(curl, connect_timeout_sec, request_timeout_sec);

    int res = tizenclaw_curl_perform(curl);
    
    long scode = 0;
    tizenclaw_curl_get_response_code(curl, &scode);
    result.status_code = static_cast<int>(scode);

    if (res != TIZENCLAW_ERROR_NONE) {
      const char* err = tizenclaw_curl_get_error_message(curl);
      result.error = err ? err : "Unknown error";
      LOG(ERROR) << "curl failed: " << result.error << " (" << (attempt + 1)
                 << "/" << max_retries << ")";
      tizenclaw_curl_destroy(curl);
      continue;
    }
    
    tizenclaw_curl_destroy(curl);

    if (result.status_code == 429 || result.status_code >= 500) {
      result.error =
          "HTTP " + std::to_string(result.status_code) + " (Retry limit)";
      LOG(WARNING) << "HTTP " << result.status_code << ", retry ("
                   << (attempt + 1) << "/" << max_retries << ")";
      continue;
    }

    result.success = (result.status_code >= 200 && result.status_code < 300);
    if (!result.success) {
      result.error = "HTTP " + std::to_string(result.status_code);
    }
    return result;
  }

  LOG(ERROR) << "All " << max_retries << " retries failed";
  result.success = false;
  return result;
}

}  // namespace tizenclaw
