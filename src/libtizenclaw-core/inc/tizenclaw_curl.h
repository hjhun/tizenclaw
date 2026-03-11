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

#ifndef TIZENCLAW_CURL_H_
#define TIZENCLAW_CURL_H_

#ifdef __cplusplus
extern "C" {
#endif

typedef struct tizenclaw_curl_s* tizenclaw_curl_h;

typedef void (*tizenclaw_curl_chunk_cb)(const char* chunk, void* user_data);

int tizenclaw_curl_create(tizenclaw_curl_h* curl);

int tizenclaw_curl_destroy(tizenclaw_curl_h curl);

int tizenclaw_curl_set_url(tizenclaw_curl_h curl, const char* url);

int tizenclaw_curl_add_header(tizenclaw_curl_h curl, const char* header);

int tizenclaw_curl_set_post_data(tizenclaw_curl_h curl, const char* data);

int tizenclaw_curl_set_method_get(tizenclaw_curl_h curl);

int tizenclaw_curl_set_timeout(tizenclaw_curl_h curl, long connect_timeout, long request_timeout);

int tizenclaw_curl_set_write_callback(
    tizenclaw_curl_h curl, tizenclaw_curl_chunk_cb callback, void* user_data);

int tizenclaw_curl_perform(tizenclaw_curl_h curl);

int tizenclaw_curl_get_response_code(tizenclaw_curl_h curl, long* code);

const char* tizenclaw_curl_get_error_message(tizenclaw_curl_h curl);

#ifdef __cplusplus
}
#endif

#endif  // TIZENCLAW_CURL_H_
