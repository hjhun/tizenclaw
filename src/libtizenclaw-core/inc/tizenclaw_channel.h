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

#ifndef TIZENCLAW_CHANNEL_H_
#define TIZENCLAW_CHANNEL_H_

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Callback invoked by a channel plugin when it receives
 *        a message from an external source.
 * @since_tizen 10.0
 * @param[in] session_id  Session identifier for the message.
 * @param[in] text        The message text.
 * @param[in] user_data   User data passed during start.
 * @return The response text. The caller must free() the
 *         returned string.
 */
typedef char* (*tizenclaw_channel_prompt_cb)(
    const char* session_id, const char* text,
    void* user_data);

/**
 * @brief Initializes the channel plugin.
 * @since_tizen 10.0
 * @param[in] config_json  JSON configuration string.
 * @return @c true on success, @c false otherwise.
 */
bool TIZENCLAW_CHANNEL_INITIALIZE(const char* config_json);

/**
 * @brief Gets the channel plugin name.
 * @since_tizen 10.0
 * @return The name string (e.g. "my_custom_channel").
 *         Must remain valid until shutdown.
 */
const char* TIZENCLAW_CHANNEL_GET_NAME(void);

/**
 * @brief Starts the channel plugin.
 * @since_tizen 10.0
 * @param[in] cb         Callback to invoke when a message
 *                       arrives. The plugin calls this to
 *                       forward messages to the agent.
 * @param[in] user_data  Opaque pointer passed back through
 *                       the callback.
 * @return @c true on success, @c false otherwise.
 */
bool TIZENCLAW_CHANNEL_START(
    tizenclaw_channel_prompt_cb cb, void* user_data);

/**
 * @brief Stops and cleans up the channel plugin.
 * @since_tizen 10.0
 */
void TIZENCLAW_CHANNEL_STOP(void);

#ifdef __cplusplus
}
#endif

#endif  // TIZENCLAW_CHANNEL_H_
