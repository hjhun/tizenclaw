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

#ifndef API_TIZENCLAW_ERROR_H_
#define API_TIZENCLAW_ERROR_H_

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Enumeration for TizenClaw error codes.
 * @since_tizen 10.0
 */
typedef enum {
  TIZENCLAW_ERROR_NONE = 0,                   /**< Successful */
  TIZENCLAW_ERROR_INVALID_PARAMETER = -1,     /**< Invalid parameter */
  TIZENCLAW_ERROR_OUT_OF_MEMORY = -2,         /**< Out of memory */
  TIZENCLAW_ERROR_CONNECTION_REFUSED = -3,     /**< Connection refused */
  TIZENCLAW_ERROR_IO_ERROR = -4,              /**< I/O error */
  TIZENCLAW_ERROR_NOT_SUPPORTED = -5,         /**< Not supported */
  TIZENCLAW_ERROR_COMMUNICATION_FAILED = -6,  /**< Communication failed */
} tizenclaw_error_e;

#ifdef __cplusplus
}
#endif

#endif  /* API_TIZENCLAW_ERROR_H_ */
