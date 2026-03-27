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

#ifndef TIZENCLAW_TOOL_EXECUTOR_TOOL_HANDLER_HH_
#define TIZENCLAW_TOOL_EXECUTOR_TOOL_HANDLER_HH_

#include <string>

#include <json.hpp>

#include "python_engine.hh"

namespace tizenclaw {
namespace tool_executor {

class ToolHandler {
 public:
  explicit ToolHandler(PythonEngine& python_engine);

  nlohmann::json HandleTool(const std::string& tool_name,
                             const std::string& args);

 private:
  std::pair<std::string, std::string> DetectRuntime(
      const std::string& tool_name);
  std::string FindToolScript(const std::string& tool_name,
                              const std::string& entry_point);

  PythonEngine& python_engine_;
};

}  // namespace tool_executor
}  // namespace tizenclaw

#endif  // TIZENCLAW_TOOL_EXECUTOR_TOOL_HANDLER_HH_
