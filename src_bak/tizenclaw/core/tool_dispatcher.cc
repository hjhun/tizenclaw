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
#include "tool_dispatcher.hh"

#include "../../common/logging.hh"

namespace tizenclaw {

void ToolDispatcher::Register(
    const std::string& name, ToolHandler handler) {
  std::lock_guard<std::mutex> lock(mutex_);
  handlers_[name] = std::move(handler);
}

void ToolDispatcher::Unregister(
    const std::string& name) {
  std::lock_guard<std::mutex> lock(mutex_);
  handlers_.erase(name);
}

std::string ToolDispatcher::Execute(
    const std::string& name,
    const nlohmann::json& args,
    const std::string& session_id) {
  ToolHandler handler;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    auto it = handlers_.find(name);
    if (it == handlers_.end()) {
      LOG(WARNING) << "ToolDispatcher: unknown tool '"
                   << name << "'";
      return "{\"error\": \"Unknown tool: " +
             name + "\"}";
    }
    handler = it->second;
  }
  // Execute outside lock to avoid deadlock
  return handler(args, name, session_id);
}

std::vector<std::string>
ToolDispatcher::ListTools() const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<std::string> names;
  names.reserve(handlers_.size());
  for (const auto& [name, handler] : handlers_)
    names.push_back(name);
  return names;
}

bool ToolDispatcher::HasTool(
    const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  return handlers_.contains(name);
}

size_t ToolDispatcher::Size() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return handlers_.size();
}

}  // namespace tizenclaw
