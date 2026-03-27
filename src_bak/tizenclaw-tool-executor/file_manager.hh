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

#ifndef TIZENCLAW_TOOL_EXECUTOR_FILE_MANAGER_HH_
#define TIZENCLAW_TOOL_EXECUTOR_FILE_MANAGER_HH_

#include <mutex>

#include <json.hpp>

namespace tizenclaw {
namespace tool_executor {

class FileManager {
 public:
  nlohmann::json Handle(const nlohmann::json& req);
 private:
  std::mutex mutex_;
};

}  // namespace tool_executor
}  // namespace tizenclaw

#endif  // TIZENCLAW_TOOL_EXECUTOR_FILE_MANAGER_HH_
