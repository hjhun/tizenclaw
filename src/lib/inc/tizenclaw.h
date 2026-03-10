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

#ifndef API_TIZENCLAW_H_
#define API_TIZENCLAW_H_

#include <tizenclaw_error.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief The TizenClaw client handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_client_h;

/**
 * @brief Called when a single response is received from the TizenClaw daemon.
 * @since_tizen 10.0
 * @param[in] session_id The session ID associated with the request.
 * @param[in] response The response string.
 * @param[in] user_data The user data passed from the request function.
 */
typedef void (*tizenclaw_response_cb)(const char *session_id, const char *response, void *user_data);

/**
 * @brief Called when a stream chunk is received from the TizenClaw daemon.
 * @since_tizen 10.0
 * @param[in] session_id The session ID associated with the request.
 * @param[in] chunk The chunk string received.
 * @param[in] is_done True if the stream has finished, false otherwise.
 * @param[in] user_data The user data passed from the request function.
 */
typedef void (*tizenclaw_stream_cb)(const char *session_id, const char *chunk, bool is_done, void *user_data);

/**
 * @brief Called when an error occurs during a request.
 * @since_tizen 10.0
 * @param[in] session_id The session ID associated with the errant request.
 * @param[in] error_code The error code.
 * @param[in] error_message The error message.
 * @param[in] user_data The user data passed from the request function.
 */
typedef void (*tizenclaw_error_cb)(const char *session_id, int error_code, const char *error_message, void *user_data);

/**
 * @brief Creates a TizenClaw client handle.
 * @since_tizen 10.0
 * @param[out] client The TizenClaw client handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 * @retval #TIZENCLAW_ERROR_OUT_OF_MEMORY Out of memory
 */
int tizenclaw_client_create(tizenclaw_client_h *client);

/**
 * @brief Destroys the TizenClaw client handle.
 * @since_tizen 10.0
 * @param[in] client The TizenClaw client handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_client_destroy(tizenclaw_client_h client);

/**
 * @brief Sends a request to the TizenClaw daemon asynchronously.
 * @since_tizen 10.0
 * @param[in] client The TizenClaw client handle.
 * @param[in] session_id The session ID (can be NULL or empty for default).
 * @param[in] prompt The prompt text to send.
 * @param[in] response_cb The callback function to be invoked when the response is ready.
 * @param[in] error_cb The callback function to be invoked on error.
 * @param[in] user_data The user data to be passed to the callbacks.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 * @retval #TIZENCLAW_ERROR_CONNECTION_REFUSED Daemon not reachable
 */
int tizenclaw_client_send_request(tizenclaw_client_h client, const char *session_id, const char *prompt,
                                  tizenclaw_response_cb response_cb, tizenclaw_error_cb error_cb, void *user_data);

/**
 * @brief Sends a streaming request to the TizenClaw daemon asynchronously.
 * @since_tizen 10.0
 * @param[in] client The TizenClaw client handle.
 * @param[in] session_id The session ID (can be NULL or empty for default).
 * @param[in] prompt The prompt text to send.
 * @param[in] stream_cb The callback function to be invoked for each chunk.
 * @param[in] error_cb The callback function to be invoked on error.
 * @param[in] user_data The user data to be passed to the callbacks.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 * @retval #TIZENCLAW_ERROR_CONNECTION_REFUSED Daemon not reachable
 */
int tizenclaw_client_send_request_stream(tizenclaw_client_h client, const char *session_id, const char *prompt,
                                         tizenclaw_stream_cb stream_cb, tizenclaw_error_cb error_cb, void *user_data);

#ifdef __cplusplus
}
#endif

#endif  // API_TIZENCLAW_H_
