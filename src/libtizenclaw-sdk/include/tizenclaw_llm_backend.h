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

/**
 * @brief The LLM response handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_response_h;

/**
 * @brief The LLM messages list handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_messages_h;

/**
 * @brief The LLM message handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_message_h;

/**
 * @brief The LLM tools list handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_tools_h;

/**
 * @brief The LLM tool handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_tool_h;

/**
 * @brief The LLM tool call handle.
 * @since_tizen 10.0
 */
typedef void* tizenclaw_llm_tool_call_h;

/**
 * @brief Callback for streaming chunks.
 * @since_tizen 10.0
 * @param[in] chunk The text chunk.
 * @param[in] user_data User data context passed from the caller.
 */
typedef void (*tizenclaw_llm_backend_chunk_cb)(const char* chunk,
                                               void* user_data);

/**
 * @brief Callback for iterating over tool calls in a response or message.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[in] user_data User data.
 * @return @c true to continue, @c false to stop.
 */
typedef bool (*tizenclaw_llm_tool_call_cb)(tizenclaw_llm_tool_call_h tool_call,
                                           void* user_data);

/**
 * @brief Callback for iterating over messages.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] user_data User data.
 * @return @c true to continue, @c false to stop.
 */
typedef bool (*tizenclaw_llm_message_cb)(tizenclaw_llm_message_h message,
                                         void* user_data);

/**
 * @brief Callback for iterating over tools.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[in] user_data User data.
 * @return @c true to continue, @c false to stop.
 */
typedef bool (*tizenclaw_llm_tool_cb)(tizenclaw_llm_tool_h tool,
                                      void* user_data);

/**
 * @brief Creates a tool call handle.
 * @since_tizen 10.0
 * @param[out] tool_call The tool call handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_create(tizenclaw_llm_tool_call_h* tool_call);

/**
 * @brief Destroys a tool call handle.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_destroy(tizenclaw_llm_tool_call_h tool_call);

/**
 * @brief Sets the ID of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[in] id The ID string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_set_id(tizenclaw_llm_tool_call_h tool_call,
                                   const char* id);

/**
 * @brief Gets the ID of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[out] id The ID string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_get_id(tizenclaw_llm_tool_call_h tool_call,
                                   char** id);

/**
 * @brief Sets the name of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[in] name The name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_set_name(tizenclaw_llm_tool_call_h tool_call,
                                     const char* name);

/**
 * @brief Gets the name of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[out] name The name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_get_name(tizenclaw_llm_tool_call_h tool_call,
                                     char** name);

/**
 * @brief Sets the arguments JSON of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[in] args_json The arguments JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_set_args_json(tizenclaw_llm_tool_call_h tool_call,
                                          const char* args_json);

/**
 * @brief Gets the arguments JSON of a tool call.
 * @since_tizen 10.0
 * @param[in] tool_call The tool call handle.
 * @param[out] args_json The arguments JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_call_get_args_json(tizenclaw_llm_tool_call_h tool_call,
                                          char** args_json);

/**
 * @brief Creates a message handle.
 * @since_tizen 10.0
 * @param[out] message The message handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_create(tizenclaw_llm_message_h* message);

/**
 * @brief Destroys a message handle.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_destroy(tizenclaw_llm_message_h message);

/**
 * @brief Sets the role of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] role The role string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_set_role(tizenclaw_llm_message_h message,
                                   const char* role);

/**
 * @brief Gets the role of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[out] role The role string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_get_role(tizenclaw_llm_message_h message,
                                   char** role);

/**
 * @brief Sets the text of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] text The text string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_set_text(tizenclaw_llm_message_h message,
                                   const char* text);

/**
 * @brief Gets the text of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[out] text The text string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_get_text(tizenclaw_llm_message_h message,
                                   char** text);

/**
 * @brief Adds a tool call to a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] tool_call The tool call handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_add_tool_call(tizenclaw_llm_message_h message,
                                        tizenclaw_llm_tool_call_h tool_call);

/**
 * @brief Iterates over tool calls of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] callback The callback function.
 * @param[in] user_data User data for the callback.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_foreach_tool_calls(
    tizenclaw_llm_message_h message, tizenclaw_llm_tool_call_cb callback,
    void* user_data);

/**
 * @brief Sets the tool name of a message (for tool results).
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] tool_name The tool name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_set_tool_name(tizenclaw_llm_message_h message,
                                        const char* tool_name);

/**
 * @brief Gets the tool name of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[out] tool_name The tool name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_get_tool_name(tizenclaw_llm_message_h message,
                                        char** tool_name);

/**
 * @brief Sets the tool call ID of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] tool_call_id The tool call ID string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_set_tool_call_id(tizenclaw_llm_message_h message,
                                           const char* tool_call_id);

/**
 * @brief Gets the tool call ID of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[out] tool_call_id The tool call ID string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_get_tool_call_id(tizenclaw_llm_message_h message,
                                           char** tool_call_id);

/**
 * @brief Sets the tool result JSON of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[in] tool_result_json The JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_set_tool_result_json(tizenclaw_llm_message_h message,
                                               const char* tool_result_json);

/**
 * @brief Gets the tool result JSON of a message.
 * @since_tizen 10.0
 * @param[in] message The message handle.
 * @param[out] tool_result_json The JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_message_get_tool_result_json(tizenclaw_llm_message_h message,
                                               char** tool_result_json);

/**
 * @brief Creates a messages list handle.
 * @since_tizen 10.0
 * @param[out] messages The messages list handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_messages_create(tizenclaw_llm_messages_h* messages);

/**
 * @brief Destroys a messages list handle.
 * @since_tizen 10.0
 * @param[in] messages The messages list handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_messages_destroy(tizenclaw_llm_messages_h messages);

/**
 * @brief Adds a message to the messages list.
 * @since_tizen 10.0
 * @param[in] messages The messages list handle.
 * @param[in] message The message handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_messages_add(tizenclaw_llm_messages_h messages,
                               tizenclaw_llm_message_h message);

/**
 * @brief Iterates over messages in a list.
 * @since_tizen 10.0
 * @param[in] messages The messages list handle.
 * @param[in] callback The callback function.
 * @param[in] user_data User data for the callback.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_messages_foreach(tizenclaw_llm_messages_h messages,
                                   tizenclaw_llm_message_cb callback,
                                   void* user_data);

/**
 * @brief Creates a tool handle.
 * @since_tizen 10.0
 * @param[out] tool The tool handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_create(tizenclaw_llm_tool_h* tool);

/**
 * @brief Destroys a tool handle.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_destroy(tizenclaw_llm_tool_h tool);

/**
 * @brief Sets the name of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[in] name The name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_set_name(tizenclaw_llm_tool_h tool, const char* name);

/**
 * @brief Gets the name of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[out] name The name string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_get_name(tizenclaw_llm_tool_h tool, char** name);

/**
 * @brief Sets the description of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[in] description The description string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_set_description(tizenclaw_llm_tool_h tool,
                                       const char* description);

/**
 * @brief Gets the description of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[out] description The description string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_get_description(tizenclaw_llm_tool_h tool,
                                       char** description);

/**
 * @brief Sets the parameters JSON of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[in] parameters_json The parameters JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_set_parameters_json(tizenclaw_llm_tool_h tool,
                                           const char* parameters_json);

/**
 * @brief Gets the parameters JSON of a tool.
 * @since_tizen 10.0
 * @param[in] tool The tool handle.
 * @param[out] parameters_json The parameters JSON string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tool_get_parameters_json(tizenclaw_llm_tool_h tool,
                                           char** parameters_json);

/**
 * @brief Creates a tools list handle.
 * @since_tizen 10.0
 * @param[out] tools The tools list handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tools_create(tizenclaw_llm_tools_h* tools);

/**
 * @brief Destroys a tools list handle.
 * @since_tizen 10.0
 * @param[in] tools The tools list handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tools_destroy(tizenclaw_llm_tools_h tools);

/**
 * @brief Adds a tool to the tools list.
 * @since_tizen 10.0
 * @param[in] tools The tools list handle.
 * @param[in] tool The tool handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tools_add(tizenclaw_llm_tools_h tools,
                            tizenclaw_llm_tool_h tool);

/**
 * @brief Iterates over tools in a list.
 * @since_tizen 10.0
 * @param[in] tools The tools list handle.
 * @param[in] callback The callback function.
 * @param[in] user_data User data for the callback.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_tools_foreach(tizenclaw_llm_tools_h tools,
                                tizenclaw_llm_tool_cb callback,
                                void* user_data);

/**
 * @brief Creates a response handle.
 * @since_tizen 10.0
 * @param[out] response The response handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_create(tizenclaw_llm_response_h* response);

/**
 * @brief Destroys a response handle.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_destroy(tizenclaw_llm_response_h response);

/**
 * @brief Sets the success flag of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] success The success flag.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_success(tizenclaw_llm_response_h response,
                                       bool success);

/**
 * @brief Checks if a response is a success.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] success The success flag.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_is_success(tizenclaw_llm_response_h response,
                                      bool* success);

/**
 * @brief Sets the text of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] text The text string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_text(tizenclaw_llm_response_h response,
                                    const char* text);

/**
 * @brief Gets the text of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] text The text string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_text(tizenclaw_llm_response_h response,
                                    char** text);

/**
 * @brief Sets the error message of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] error_message The error message string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_error_message(tizenclaw_llm_response_h response,
                                             const char* error_message);

/**
 * @brief Gets the error message of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] error_message The error message string.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_error_message(tizenclaw_llm_response_h response,
                                             char** error_message);

/**
 * @brief Adds a tool call to a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] tool_call The tool call handle.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_add_llm_tool_call(
    tizenclaw_llm_response_h response, tizenclaw_llm_tool_call_h tool_call);

/**
 * @brief Iterates over tool calls of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] callback The callback function.
 * @param[in] user_data User data for the callback.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_foreach_llm_tool_calls(
    tizenclaw_llm_response_h response, tizenclaw_llm_tool_call_cb callback,
    void* user_data);

/**
 * @brief Sets the prompt tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] prompt_tokens The prompt tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_prompt_tokens(tizenclaw_llm_response_h response,
                                             int prompt_tokens);

/**
 * @brief Gets the prompt tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] prompt_tokens The prompt tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_prompt_tokens(tizenclaw_llm_response_h response,
                                             int* prompt_tokens);

/**
 * @brief Sets the completion tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] completion_tokens The completion tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_completion_tokens(
    tizenclaw_llm_response_h response, int completion_tokens);

/**
 * @brief Gets the completion tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] completion_tokens The completion tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_completion_tokens(
    tizenclaw_llm_response_h response, int* completion_tokens);

/**
 * @brief Sets the total tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] total_tokens The total tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_total_tokens(tizenclaw_llm_response_h response,
                                            int total_tokens);

/**
 * @brief Gets the total tokens of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] total_tokens The total tokens count.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_total_tokens(tizenclaw_llm_response_h response,
                                            int* total_tokens);

/**
 * @brief Sets the HTTP status of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[in] http_status The HTTP status code.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_set_http_status(tizenclaw_llm_response_h response,
                                           int http_status);

/**
 * @brief Gets the HTTP status of a response.
 * @since_tizen 10.0
 * @param[in] response The response handle.
 * @param[out] http_status The HTTP status code.
 * @return @c 0 on success, otherwise a negative error value.
 * @retval #TIZENCLAW_ERROR_NONE Successful
 * @retval #TIZENCLAW_ERROR_INVALID_PARAMETER Invalid parameter
 */
int tizenclaw_llm_response_get_http_status(tizenclaw_llm_response_h response,
                                           int* http_status);

/**
 * @brief Initializes the backend plugin.
 * @since_tizen 10.0
 * @param[in] config_json_str The configuration JSON string.
 * @return @c true on success, @c false otherwise.
 */
bool TIZENCLAW_LLM_BACKEND_INITIALIZE(const char* config_json_str);

/**
 * @brief Gets the backend plugin name.
 * @since_tizen 10.0
 * @return The name string.
 */
const char* TIZENCLAW_LLM_BACKEND_GET_NAME(void);

/**
 * @brief Performs a chat request with the backend plugin.
 * @since_tizen 10.0
 * @param[in] messages The messages list handle.
 * @param[in] tools The tools list handle.
 * @param[in] on_chunk The callback for text chunks.
 * @param[in] user_data User data for the callback.
 * @param[in] system_prompt The system prompt string.
 * @return The response handle, or @c NULL on error.
 */
tizenclaw_llm_response_h TIZENCLAW_LLM_BACKEND_CHAT(
    tizenclaw_llm_messages_h messages, tizenclaw_llm_tools_h tools,
    tizenclaw_llm_backend_chunk_cb on_chunk, void* user_data,
    const char* system_prompt);

/**
 * @brief Shuts down the backend plugin.
 * @since_tizen 10.0
 */
void TIZENCLAW_LLM_BACKEND_SHUTDOWN(void);

#ifdef __cplusplus
}
#endif

#endif  // TIZENCLAW_LLM_BACKEND_H_
