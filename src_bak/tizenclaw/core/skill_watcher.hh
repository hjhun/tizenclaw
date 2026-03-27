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
#ifndef SKILL_WATCHER_HH
#define SKILL_WATCHER_HH

#include <atomic>
#include <functional>
#include <map>
#include <mutex>
#include <string>
#include <thread>

namespace tizenclaw {

// Watches /opt/usr/share/tizen-tools/skills/ for
// manifest.json changes using Linux inotify.
// Calls a user-provided callback when skills
// need reloading.
class SkillWatcher {
 public:
  using ReloadCallback = std::function<void()>;

  SkillWatcher();
  ~SkillWatcher();

  // Start watching the given directory.
  // callback is invoked when manifest changes
  // are detected (debounced 500ms).
  bool Start(const std::string& skills_dir, ReloadCallback callback);

  // Stop watching and join the thread.
  void Stop();

  bool IsRunning() const { return running_.load(); }

 private:
  // Watch thread entry point
  void WatchLoop();

  // Add inotify watch for a subdirectory
  void AddSubdirWatch(const std::string& path);

  // Scan and add watches for all subdirs
  void ScanSubdirectories();

  int inotify_fd_ = -1;
  std::string skills_dir_;
  ReloadCallback callback_;
  std::atomic<bool> running_{false};
  std::thread watch_thread_;

  // Map: watch descriptor -> directory path
  std::map<int, std::string> wd_to_path_;
  std::mutex wd_mutex_;
};

}  // namespace tizenclaw

#endif  // SKILL_WATCHER_HH
