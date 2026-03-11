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
#include "tizenclaw_llm_backend.h" // For TIZENCLAW_ERROR_* constants

#include <curl/curl.h>
#include <unistd.h>
#include <string.h>
#include <dlog.h>

#define LOG_TAG "TIZENCLAW_CURL"

struct tizenclaw_curl_s {
  CURL* curl;
  struct curl_slist* headers;
  char errbuf[CURL_ERROR_SIZE];
  long response_code;
  tizenclaw_curl_chunk_cb user_cb;
  void* user_data;

  tizenclaw_curl_s() {
    curl = nullptr;
    headers = nullptr;
    memset(errbuf, 0, sizeof(errbuf));
    response_code = 0;
    user_cb = nullptr;
    user_data = nullptr;
  }

  ~tizenclaw_curl_s() {
    if (headers) {
      curl_slist_free_all(headers);
    }
    if (curl) {
      curl_easy_cleanup(curl);
    }
  }

  static size_t WriteCallback(void* contents, size_t size, size_t nmemb,
                              void* userp) {
    size_t total_size = size * nmemb;
    tizenclaw_curl_s* ctx = static_cast<tizenclaw_curl_s*>(userp);

    if (ctx && ctx->user_cb) {
      char* null_terminated = new char[total_size + 1];
      memcpy(null_terminated, contents, total_size);
      null_terminated[total_size] = '\0';
      ctx->user_cb(null_terminated, ctx->user_data);
      delete[] null_terminated;
    }

    return total_size;
  }
};

int tizenclaw_curl_create(tizenclaw_curl_h* curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;

  tizenclaw_curl_h instance = new tizenclaw_curl_s();
  instance->curl = curl_easy_init();
  if (!instance->curl) {
    delete instance;
    return TIZENCLAW_ERROR_IO_ERROR;
  }

  curl_easy_setopt(instance->curl, CURLOPT_ERRORBUFFER, instance->errbuf);
  curl_easy_setopt(instance->curl, CURLOPT_SSL_VERIFYPEER, 1L);
  curl_easy_setopt(instance->curl, CURLOPT_SSL_VERIFYHOST, 2L);

  const char* ca_paths[] = {
      "/etc/ssl/certs/ca-certificates.crt", "/etc/ssl/ca-bundle.pem",
      "/etc/pki/tls/certs/ca-bundle.crt",
      "/usr/share/ca-certificates/ca-bundle.crt", nullptr};

  for (int i = 0; ca_paths[i]; ++i) {
    if (access(ca_paths[i], R_OK) == 0) {
      curl_easy_setopt(instance->curl, CURLOPT_CAINFO, ca_paths[i]);
      break;
    }
  }

  *curl = instance;
  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_destroy(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  delete curl;
  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_set_url(tizenclaw_curl_h curl, const char* url) {
  if (!curl || !url) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  CURLcode res = curl_easy_setopt(curl->curl, CURLOPT_URL, url);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

int tizenclaw_curl_add_header(tizenclaw_curl_h curl, const char* header) {
  if (!curl || !header) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  curl->headers = curl_slist_append(curl->headers, header);
  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_set_post_data(tizenclaw_curl_h curl, const char* data) {
  if (!curl || !data) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  CURLcode res = curl_easy_setopt(curl->curl, CURLOPT_POSTFIELDS, data);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

int tizenclaw_curl_set_method_get(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  CURLcode res = curl_easy_setopt(curl->curl, CURLOPT_HTTPGET, 1L);
  return (res == CURLE_OK) ? TIZENCLAW_ERROR_NONE : TIZENCLAW_ERROR_IO_ERROR;
}

int tizenclaw_curl_set_timeout(tizenclaw_curl_h curl, long connect_timeout, long request_timeout) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  curl_easy_setopt(curl->curl, CURLOPT_CONNECTTIMEOUT, connect_timeout);
  curl_easy_setopt(curl->curl, CURLOPT_TIMEOUT, request_timeout);
  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_set_write_callback(
    tizenclaw_curl_h curl, tizenclaw_curl_chunk_cb callback, void* user_data) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  curl->user_cb = callback;
  curl->user_data = user_data;
  curl_easy_setopt(curl->curl, CURLOPT_WRITEFUNCTION, tizenclaw_curl_s::WriteCallback);
  curl_easy_setopt(curl->curl, CURLOPT_WRITEDATA, curl);
  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_perform(tizenclaw_curl_h curl) {
  if (!curl) return TIZENCLAW_ERROR_INVALID_PARAMETER;

  if (curl->headers) {
    curl_easy_setopt(curl->curl, CURLOPT_HTTPHEADER, curl->headers);
  }

  curl->errbuf[0] = 0;
  CURLcode res = curl_easy_perform(curl->curl);

  curl_easy_getinfo(curl->curl, CURLINFO_RESPONSE_CODE, &curl->response_code);

  if (res != CURLE_OK) {
    const char* err = curl->errbuf[0] ? curl->errbuf : curl_easy_strerror(res);
    dlog_print(DLOG_ERROR, LOG_TAG, "CURL perform error: %s", err);
    return TIZENCLAW_ERROR_IO_ERROR;
  }

  return TIZENCLAW_ERROR_NONE;
}

int tizenclaw_curl_get_response_code(tizenclaw_curl_h curl, long* code) {
  if (!curl || !code) return TIZENCLAW_ERROR_INVALID_PARAMETER;
  *code = curl->response_code;
  return TIZENCLAW_ERROR_NONE;
}

const char* tizenclaw_curl_get_error_message(tizenclaw_curl_h curl) {
  if (!curl) return NULL;
  return curl->errbuf[0] ? curl->errbuf : "Unknown or no error";
}
