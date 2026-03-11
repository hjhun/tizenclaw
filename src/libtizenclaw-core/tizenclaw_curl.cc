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

#include "tizenclaw_curl.h"

#include <curl/curl.h>
#include <unistd.h>

#include "logging.hh"

#undef EXPORT
#define EXPORT __attribute__((visibility("default")))

#undef API
#define API extern "C" EXPORT

namespace {

struct TizenClawCurl {
  CURL* curl_;
  struct curl_slist* headers_;
  char errbuf_[CURL_ERROR_SIZE];
  long response_code_;
  tizenclaw_curl_chunk_cb user_cb_;
  void* user_data_;

  TizenClawCurl() {
    curl_ = nullptr;
    headers_ = nullptr;
    memset(errbuf_, 0, sizeof(errbuf_));
    response_code_ = 0;
    user_cb_ = nullptr;
    user_data_ = nullptr;
  }

  ~TizenClawCurl() {
    if (headers_) {
      curl_slist_free_all(headers_);
    }
    if (curl_) {
      curl_easy_cleanup(curl_);
    }
  }

  static size_t WriteCallback(void* contents, size_t size, size_t nmemb,
                              void* userp) {
    size_t total_size = size * nmemb;
    TizenClawCurl* ctx = static_cast<TizenClawCurl*>(userp);

    if (ctx && ctx->user_cb_) {
      char* null_terminated = new char[total_size + 1];
      memcpy(null_terminated, contents, total_size);
      null_terminated[total_size] = '\0';
      ctx->user_cb_(null_terminated, ctx->user_data_);
      delete[] null_terminated;
    }

    return total_size;
  }
};

}  // namespace

API int tizenclaw_curl_create(tizenclaw_curl_h* curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;

  TizenClawCurl* instance = new TizenClawCurl();
  instance->curl_ = curl_easy_init();
  if (!instance->curl_) {
    delete instance;
    return TIZENCLAW_ERROR_IO_ERROR;
  }

  curl_easy_setopt(instance->curl_, CURLOPT_ERRORBUFFER, instance->errbuf_);
  curl_easy_setopt(instance->curl_, CURLOPT_SSL_VERIFYPEER, 1L);
  curl_easy_setopt(instance->curl_, CURLOPT_SSL_VERIFYHOST, 2L);

  const char* ca_paths[] = {
      "/etc/ssl/certs/ca-certificates.crt", "/etc/ssl/ca-bundle.pem",
      "/etc/pki/tls/certs/ca-bundle.crt",
      "/usr/share/ca-certificates/ca-bundle.crt", nullptr};

  for (int i = 0; ca_paths[i]; ++i) {
    if (access(ca_paths[i], R_OK) == 0) {
      curl_easy_setopt(instance->curl_, CURLOPT_CAINFO, ca_paths[i]);
      break;
    }
  }

  *curl = instance;
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_destroy(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  delete instance;
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_set_url(tizenclaw_curl_h curl, const char* url) {
  if (!curl || !url) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  CURLcode res = curl_easy_setopt(instance->curl_, CURLOPT_URL, url);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

API int tizenclaw_curl_add_header(tizenclaw_curl_h curl, const char* header) {
  if (!curl || !header) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  instance->headers_ = curl_slist_append(instance->headers_, header);
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_set_post_data(tizenclaw_curl_h curl, const char* data) {
  if (!curl || !data) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  CURLcode res = curl_easy_setopt(instance->curl_, CURLOPT_POSTFIELDS, data);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

API int tizenclaw_curl_set_method_get(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  CURLcode res = curl_easy_setopt(instance->curl_, CURLOPT_HTTPGET, 1L);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

API int tizenclaw_curl_set_timeout(tizenclaw_curl_h curl, long connect_timeout, long request_timeout) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  curl_easy_setopt(instance->curl_, CURLOPT_CONNECTTIMEOUT, connect_timeout);
  curl_easy_setopt(instance->curl_, CURLOPT_TIMEOUT, request_timeout);
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_set_write_callback(
    tizenclaw_curl_h curl, tizenclaw_curl_chunk_cb callback, void* user_data) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  instance->user_cb_ = callback;
  instance->user_data_ = user_data;
  curl_easy_setopt(instance->curl_, CURLOPT_WRITEFUNCTION, TizenClawCurl::WriteCallback);
  curl_easy_setopt(instance->curl_, CURLOPT_WRITEDATA, instance);
  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_perform(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);

  if (instance->headers_) {
    curl_easy_setopt(instance->curl_, CURLOPT_HTTPHEADER, instance->headers_);
  }

  instance->errbuf_[0] = 0;
  CURLcode res = curl_easy_perform(instance->curl_);

  curl_easy_getinfo(instance->curl_, CURLINFO_RESPONSE_CODE, &instance->response_code_);

  if (res != CURLE_OK) {
    const char* err = instance->errbuf_[0] ? instance->errbuf_ : curl_easy_strerror(res);
    LOG(ERROR) << "CURL perform error: " << err;
    return TIZENCLAW_ERROR_IO_ERROR;
  }

  return TIZENCLAW_ERROR_NONE;
}

API int tizenclaw_curl_get_response_code(tizenclaw_curl_h curl, long* code) {
  if (!curl || !code) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  *code = instance->response_code_;
  return TIZENCLAW_ERROR_NONE;
}

API const char* tizenclaw_curl_get_error_message(tizenclaw_curl_h curl) {
  if (!curl) return NULL;
  TizenClawCurl* instance = static_cast<TizenClawCurl*>(curl);
  return instance->errbuf_[0] ? instance->errbuf_ : "Unknown or no error";
}
