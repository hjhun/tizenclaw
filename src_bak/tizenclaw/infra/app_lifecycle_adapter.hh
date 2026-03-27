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
#ifndef APP_LIFECYCLE_ADAPTER_HH
#define APP_LIFECYCLE_ADAPTER_HH

#include <string>

#include "event_adapter.hh"

namespace tizenclaw {

// Wraps AUL app lifecycle API to monitor
// application state changes across the system.
class AppLifecycleAdapter : public IEventAdapter {
 public:
  AppLifecycleAdapter() = default;
  ~AppLifecycleAdapter() override;

  void Start() override;
  void Stop() override;
  [[nodiscard]] std::string GetName() const override;

 private:
  // Static callback for AUL lifecycle API
  static void OnStateChanged(
      const char* app_id,
      pid_t pid,
      int state,
      bool has_focus,
      void* user_data);

  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // APP_LIFECYCLE_ADAPTER_HH
