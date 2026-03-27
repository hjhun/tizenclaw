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

#ifndef TIZENCLAW_TOOL_EXECUTOR_PYTHON_ENGINE_HH_
#define TIZENCLAW_TOOL_EXECUTOR_PYTHON_ENGINE_HH_

#include <functional>
#include <mutex>
#include <string>
#include <utility>

namespace tizenclaw {
namespace tool_executor {

class PythonEngine {
 public:
  ~PythonEngine();
  bool Initialize();
  bool IsInitialized() const { return initialized_; }
  std::pair<std::string, int> RunCode(const std::string& code);
  static std::string FindPython3();

 private:
  std::mutex mutex_;
  bool initialized_ = false;
};

}  // namespace tool_executor
}  // namespace tizenclaw

#endif  // TIZENCLAW_TOOL_EXECUTOR_PYTHON_ENGINE_HH_
