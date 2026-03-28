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

/**
 * @file tizenclaw.h
 * @brief TizenClaw Agent C API
 *
 * Provides C-compatible interface to the TizenClaw AI agent system.
 * Internal implementation is in Rust; this header exposes the FFI boundary.
 *
 * @section Usage
 * @code
 * #include <tizenclaw/tizenclaw.h>
 *
 * tizenclaw_h agent;
 * int ret = tizenclaw_create(&agent);
 * if (ret != TIZENCLAW_ERROR_NONE) { ... }
 *
 * ret = tizenclaw_initialize(agent);
 * char *response = tizenclaw_process_prompt(agent, "default", "Hello!");
 * printf("Response: %s\n", response);
 * tizenclaw_free_string(response);
 *
 * tizenclaw_destroy(agent);
 * @endcode
 */

#ifndef __TIZENCLAW_H__
#define __TIZENCLAW_H__

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Error codes returned by TizenClaw API functions.
 */
typedef enum {
    TIZENCLAW_ERROR_NONE = 0,              /**< Success */
    TIZENCLAW_ERROR_INVALID_PARAMETER = -1, /**< Invalid parameter */
    TIZENCLAW_ERROR_OUT_OF_MEMORY = -2,     /**< Memory allocation failure */
    TIZENCLAW_ERROR_NOT_INITIALIZED = -3,   /**< Agent not initialized */
    TIZENCLAW_ERROR_ALREADY_INITIALIZED = -4, /**< Agent already initialized */
    TIZENCLAW_ERROR_IO = -5,                /**< I/O error */
    TIZENCLAW_ERROR_LLM_FAILED = -6,       /**< LLM backend failure */
    TIZENCLAW_ERROR_TOOL_FAILED = -7,      /**< Tool execution failure */
    TIZENCLAW_ERROR_NOT_SUPPORTED = -8,    /**< Operation not supported */
} tizenclaw_error_e;

/**
 * @brief Opaque handle to a TizenClaw agent instance.
 */
typedef struct tizenclaw_s *tizenclaw_h;

/**
 * @brief Callback for asynchronous prompt processing.
 *
 * @param[in] response  The complete response text (UTF-8, null-terminated)
 * @param[in] error     Error code (TIZENCLAW_ERROR_NONE on success)
 * @param[in] user_data User data passed to tizenclaw_process_prompt_async()
 */
typedef void (*tizenclaw_response_cb)(const char *response,
                                       int error,
                                       void *user_data);

/**
 * @brief Callback for streaming chunks during prompt processing.
 *
 * @param[in] chunk     A partial response chunk (UTF-8, null-terminated)
 * @param[in] user_data User data passed to tizenclaw_process_prompt_async()
 */
typedef void (*tizenclaw_stream_cb)(const char *chunk,
                                     void *user_data);

/* ═══════════════════════════════════════════
 *  Lifecycle
 * ═══════════════════════════════════════════ */

/**
 * @brief Create a new TizenClaw agent instance.
 *
 * @param[out] handle  Pointer to receive the agent handle
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_create(tizenclaw_h *handle);

/**
 * @brief Initialize the agent (loads config, LLM backends, tools).
 *
 * Must be called after tizenclaw_create() and before any other operations.
 *
 * @param[in] handle  Agent handle
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_initialize(tizenclaw_h handle);

/**
 * @brief Destroy the agent and release all resources.
 *
 * @param[in] handle  Agent handle (becomes invalid after this call)
 */
void tizenclaw_destroy(tizenclaw_h handle);

/* ═══════════════════════════════════════════
 *  Prompt Processing
 * ═══════════════════════════════════════════ */

/**
 * @brief Process a prompt synchronously.
 *
 * @param[in] handle      Agent handle
 * @param[in] session_id  Session identifier (UTF-8)
 * @param[in] prompt      User prompt text (UTF-8)
 * @return Heap-allocated response string (caller must free with tizenclaw_free_string()),
 *         or NULL on error (check tizenclaw_last_error())
 */
char *tizenclaw_process_prompt(tizenclaw_h handle,
                                const char *session_id,
                                const char *prompt);

/**
 * @brief Process a prompt asynchronously.
 *
 * @param[in] handle      Agent handle
 * @param[in] session_id  Session identifier (UTF-8)
 * @param[in] prompt      User prompt text (UTF-8)
 * @param[in] callback    Completion callback
 * @param[in] user_data   User data for callback
 * @return TIZENCLAW_ERROR_NONE on success (callback will be invoked)
 */
int tizenclaw_process_prompt_async(tizenclaw_h handle,
                                    const char *session_id,
                                    const char *prompt,
                                    tizenclaw_response_cb callback,
                                    void *user_data);

/* ═══════════════════════════════════════════
 *  Session Management
 * ═══════════════════════════════════════════ */

/**
 * @brief Clear a session's conversation history.
 *
 * @param[in] handle      Agent handle
 * @param[in] session_id  Session identifier
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_clear_session(tizenclaw_h handle, const char *session_id);

/* ═══════════════════════════════════════════
 *  Monitoring
 * ═══════════════════════════════════════════ */

/**
 * @brief Get agent status as JSON.
 *
 * @param[in] handle  Agent handle
 * @return JSON string (caller must free with tizenclaw_free_string()),
 *         or NULL on error
 */
char *tizenclaw_get_status(tizenclaw_h handle);

/**
 * @brief Get system metrics as JSON (memory, CPU, uptime, counters).
 *
 * @param[in] handle  Agent handle
 * @return JSON string (caller must free with tizenclaw_free_string()),
 *         or NULL on error
 */
char *tizenclaw_get_metrics(tizenclaw_h handle);

/* ═══════════════════════════════════════════
 *  Tools & Skills
 * ═══════════════════════════════════════════ */

/**
 * @brief Get available tools as JSON array.
 *
 * @param[in] handle  Agent handle
 * @return JSON string (caller must free with tizenclaw_free_string()),
 *         or NULL on error
 */
char *tizenclaw_get_tools(tizenclaw_h handle);

/**
 * @brief Execute a tool directly by name.
 *
 * @param[in] handle     Agent handle
 * @param[in] tool_name  Tool name (UTF-8)
 * @param[in] args_json  Tool arguments as JSON string (UTF-8)
 * @return JSON result string (caller must free with tizenclaw_free_string()),
 *         or NULL on error
 */
char *tizenclaw_execute_tool(tizenclaw_h handle,
                              const char *tool_name,
                              const char *args_json);

/**
 * @brief Force reload of skill manifests.
 *
 * @param[in] handle  Agent handle
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_reload_skills(tizenclaw_h handle);

/* ═══════════════════════════════════════════
 *  Web Dashboard
 * ═══════════════════════════════════════════ */

/**
 * @brief Start the web dashboard on the specified port.
 *
 * @param[in] handle  Agent handle
 * @param[in] port    TCP port to listen on (e.g. 9090)
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_start_dashboard(tizenclaw_h handle, uint16_t port);

/**
 * @brief Stop the web dashboard.
 *
 * @param[in] handle  Agent handle
 * @return TIZENCLAW_ERROR_NONE on success
 */
int tizenclaw_stop_dashboard(tizenclaw_h handle);

/* ═══════════════════════════════════════════
 *  Utility
 * ═══════════════════════════════════════════ */

/**
 * @brief Free a string returned by TizenClaw API functions.
 *
 * @param[in] str  String to free (NULL is safe)
 */
void tizenclaw_free_string(char *str);

/**
 * @brief Get the last error message (thread-local).
 *
 * @return Error message string (static, do NOT free), or NULL if no error
 */
const char *tizenclaw_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* __TIZENCLAW_H__ */
