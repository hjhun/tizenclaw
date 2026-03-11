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

#ifndef TIZENCLAW_LLM_BACKEND_H_
#define TIZENCLAW_LLM_BACKEND_H_

#include <stdbool.h>

#include <tizenclaw_error.h>

#ifdef __cplusplus
extern "C" {
#endif

// -----------------------------------------------------------------------------
// Opaque Handles
// -----------------------------------------------------------------------------
typedef void* tizenclaw_llm_response_h;
typedef void* tizenclaw_llm_messages_h;
typedef void* tizenclaw_llm_message_h;
typedef void* tizenclaw_llm_tools_h;
typedef void* tizenclaw_llm_tool_h;
typedef void* tizenclaw_llm_tool_call_h;


// -----------------------------------------------------------------------------
// Callbacks
// -----------------------------------------------------------------------------
/**
 * @brief Callback for streaming chunks.
 * @param chunk The text chunk.
 * @param user_data User data context passed from the caller.
 */
typedef void (*tizenclaw_llm_backend_chunk_cb)(const char* chunk,
                                               void* user_data);

/**
 * @brief Callback for iterating over tool calls in a response or message.
 * @param tool_call The tool call handle.
 * @param user_data User data.
 * @return true to continue, false to stop.
 */
typedef bool (*tizenclaw_llm_tool_call_cb)(tizenclaw_llm_tool_call_h tool_call,
                                           void* user_data);

/**
 * @brief Callback for iterating over messages.
 */
typedef bool (*tizenclaw_llm_message_cb)(tizenclaw_llm_message_h message,
                                         void* user_data);

/**
 * @brief Callback for iterating over tools.
 */
typedef bool (*tizenclaw_llm_tool_cb)(tizenclaw_llm_tool_h tool,
                                      void* user_data);

// -----------------------------------------------------------------------------
// LlmToolCall API
// -----------------------------------------------------------------------------
int tizenclaw_llm_tool_call_create(tizenclaw_llm_tool_call_h* tool_call);
int tizenclaw_llm_tool_call_destroy(tizenclaw_llm_tool_call_h tool_call);

int tizenclaw_llm_tool_call_set_id(tizenclaw_llm_tool_call_h tool_call,
                                   const char* id);
int tizenclaw_llm_tool_call_get_id(tizenclaw_llm_tool_call_h tool_call,
                                   char** id);

int tizenclaw_llm_tool_call_set_name(tizenclaw_llm_tool_call_h tool_call,
                                     const char* name);
int tizenclaw_llm_tool_call_get_name(tizenclaw_llm_tool_call_h tool_call,
                                     char** name);

int tizenclaw_llm_tool_call_set_args_json(tizenclaw_llm_tool_call_h tool_call,
                                          const char* args_json);
int tizenclaw_llm_tool_call_get_args_json(tizenclaw_llm_tool_call_h tool_call,
                                          char** args_json);

// -----------------------------------------------------------------------------
// LlmMessage API
// -----------------------------------------------------------------------------
int tizenclaw_llm_message_create(tizenclaw_llm_message_h* message);
int tizenclaw_llm_message_destroy(tizenclaw_llm_message_h message);

int tizenclaw_llm_message_set_role(tizenclaw_llm_message_h message,
                                   const char* role);
int tizenclaw_llm_message_get_role(tizenclaw_llm_message_h message,
                                   char** role);

int tizenclaw_llm_message_set_text(tizenclaw_llm_message_h message,
                                   const char* text);
int tizenclaw_llm_message_get_text(tizenclaw_llm_message_h message,
                                   char** text);

int tizenclaw_llm_message_add_tool_call(tizenclaw_llm_message_h message,
                                        tizenclaw_llm_tool_call_h tool_call);
int tizenclaw_llm_message_foreach_tool_calls(
    tizenclaw_llm_message_h message, tizenclaw_llm_tool_call_cb callback,
    void* user_data);

int tizenclaw_llm_message_set_tool_name(tizenclaw_llm_message_h message,
                                        const char* tool_name);
int tizenclaw_llm_message_get_tool_name(tizenclaw_llm_message_h message,
                                        char** tool_name);

int tizenclaw_llm_message_set_tool_call_id(tizenclaw_llm_message_h message,
                                           const char* tool_call_id);
int tizenclaw_llm_message_get_tool_call_id(tizenclaw_llm_message_h message,
                                           char** tool_call_id);

int tizenclaw_llm_message_set_tool_result_json(tizenclaw_llm_message_h message,
                                               const char* tool_result_json);
int tizenclaw_llm_message_get_tool_result_json(tizenclaw_llm_message_h message,
                                               char** tool_result_json);

// -----------------------------------------------------------------------------
// LlmMessages (List) API
// -----------------------------------------------------------------------------
int tizenclaw_llm_messages_create(tizenclaw_llm_messages_h* messages);
int tizenclaw_llm_messages_destroy(tizenclaw_llm_messages_h messages);
int tizenclaw_llm_messages_add(tizenclaw_llm_messages_h messages,
                               tizenclaw_llm_message_h message);
int tizenclaw_llm_messages_foreach(tizenclaw_llm_messages_h messages,
                                   tizenclaw_llm_message_cb callback,
                                   void* user_data);

// -----------------------------------------------------------------------------
// LlmToolDecl API
// -----------------------------------------------------------------------------
int tizenclaw_llm_tool_create(tizenclaw_llm_tool_h* tool);
int tizenclaw_llm_tool_destroy(tizenclaw_llm_tool_h tool);

int tizenclaw_llm_tool_set_name(tizenclaw_llm_tool_h tool, const char* name);
int tizenclaw_llm_tool_get_name(tizenclaw_llm_tool_h tool, char** name);

int tizenclaw_llm_tool_set_description(tizenclaw_llm_tool_h tool,
                                       const char* description);
int tizenclaw_llm_tool_get_description(tizenclaw_llm_tool_h tool,
                                       char** description);

int tizenclaw_llm_tool_set_parameters_json(tizenclaw_llm_tool_h tool,
                                           const char* parameters_json);
int tizenclaw_llm_tool_get_parameters_json(tizenclaw_llm_tool_h tool,
                                           char** parameters_json);

// -----------------------------------------------------------------------------
// LlmTools (List) API
// -----------------------------------------------------------------------------
int tizenclaw_llm_tools_create(tizenclaw_llm_tools_h* tools);
int tizenclaw_llm_tools_destroy(tizenclaw_llm_tools_h tools);
int tizenclaw_llm_tools_add(tizenclaw_llm_tools_h tools,
                            tizenclaw_llm_tool_h tool);
int tizenclaw_llm_tools_foreach(tizenclaw_llm_tools_h tools,
                                tizenclaw_llm_tool_cb callback,
                                void* user_data);

// -----------------------------------------------------------------------------
// LlmResponse API
// -----------------------------------------------------------------------------
int tizenclaw_llm_response_create(tizenclaw_llm_response_h* response);
int tizenclaw_llm_response_destroy(tizenclaw_llm_response_h response);

int tizenclaw_llm_response_set_success(tizenclaw_llm_response_h response,
                                       bool success);
int tizenclaw_llm_response_is_success(tizenclaw_llm_response_h response,
                                      bool* success);

int tizenclaw_llm_response_set_text(tizenclaw_llm_response_h response,
                                    const char* text);
int tizenclaw_llm_response_get_text(tizenclaw_llm_response_h response,
                                    char** text);

int tizenclaw_llm_response_set_error_message(tizenclaw_llm_response_h response,
                                             const char* error_message);
int tizenclaw_llm_response_get_error_message(tizenclaw_llm_response_h response,
                                             char** error_message);

int tizenclaw_llm_response_add_llm_tool_call(
    tizenclaw_llm_response_h response, tizenclaw_llm_tool_call_h tool_call);
int tizenclaw_llm_response_foreach_llm_tool_calls(
    tizenclaw_llm_response_h response, tizenclaw_llm_tool_call_cb callback,
    void* user_data);

int tizenclaw_llm_response_set_prompt_tokens(tizenclaw_llm_response_h response,
                                             int prompt_tokens);
int tizenclaw_llm_response_get_prompt_tokens(tizenclaw_llm_response_h response,
                                             int* prompt_tokens);

int tizenclaw_llm_response_set_completion_tokens(
    tizenclaw_llm_response_h response, int completion_tokens);
int tizenclaw_llm_response_get_completion_tokens(
    tizenclaw_llm_response_h response, int* completion_tokens);

int tizenclaw_llm_response_set_total_tokens(tizenclaw_llm_response_h response,
                                            int total_tokens);
int tizenclaw_llm_response_get_total_tokens(tizenclaw_llm_response_h response,
                                            int* total_tokens);

int tizenclaw_llm_response_set_http_status(tizenclaw_llm_response_h response,
                                           int http_status);
int tizenclaw_llm_response_get_http_status(tizenclaw_llm_response_h response,
                                           int* http_status);

// -----------------------------------------------------------------------------
// Plugin Exported APIs
// -----------------------------------------------------------------------------
bool TIZENCLAW_LLM_BACKEND_INITIALIZE(const char* config_json_str);
const char* TIZENCLAW_LLM_BACKEND_GET_NAME(void);
tizenclaw_llm_response_h TIZENCLAW_LLM_BACKEND_CHAT(
    tizenclaw_llm_messages_h messages, tizenclaw_llm_tools_h tools,
    tizenclaw_llm_backend_chunk_cb on_chunk, void* user_data,
    const char* system_prompt);
void TIZENCLAW_LLM_BACKEND_SHUTDOWN(void);

#ifdef __cplusplus
}
#endif

#endif  // TIZENCLAW_LLM_BACKEND_H_
