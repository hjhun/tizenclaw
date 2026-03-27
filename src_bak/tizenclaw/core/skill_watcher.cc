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
#include "skill_watcher.hh"

#include <dirent.h>
#include <poll.h>
#include <sys/inotify.h>
#include <sys/stat.h>
#include <unistd.h>

#include <chrono>
#include <cstring>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {
constexpr int kDebounceMs = 500;
constexpr uint32_t kWatchMask = IN_CREATE | IN_DELETE | IN_MODIFY | IN_MOVED_TO;
constexpr const char* kManifestFile = "manifest.json";
}  // namespace

SkillWatcher::SkillWatcher() = default;

SkillWatcher::~SkillWatcher() { Stop(); }

bool SkillWatcher::Start(const std::string& skills_dir,
                         ReloadCallback callback) {
  if (running_.load()) {
    LOG(WARNING) << "SkillWatcher already running";
    return false;
  }

  inotify_fd_ = inotify_init1(IN_NONBLOCK);
  if (inotify_fd_ < 0) {
    LOG(ERROR) << "inotify_init1 failed: " << strerror(errno);
    return false;
  }

  skills_dir_ = skills_dir;
  callback_ = std::move(callback);

  // Watch the top-level skills directory for
  // new subdirectory creation
  int wd = inotify_add_watch(inotify_fd_, skills_dir_.c_str(), kWatchMask);
  if (wd < 0) {
    LOG(ERROR) << "Failed to watch " << skills_dir_ << ": " << strerror(errno);
    close(inotify_fd_);
    inotify_fd_ = -1;
    return false;
  }

  {
    std::lock_guard<std::mutex> lock(wd_mutex_);
    wd_to_path_[wd] = skills_dir_;
  }

  // Add watches for existing subdirectories
  ScanSubdirectories();

  running_.store(true);
  watch_thread_ = std::thread(&SkillWatcher::WatchLoop, this);

  LOG(INFO) << "SkillWatcher started: " << skills_dir_;
  return true;
}

void SkillWatcher::Stop() {
  if (!running_.load()) return;

  running_.store(false);

  if (watch_thread_.joinable()) {
    watch_thread_.join();
  }

  if (inotify_fd_ >= 0) {
    // Remove all watches
    std::lock_guard<std::mutex> lock(wd_mutex_);
    for (auto& [wd, path] : wd_to_path_) {
      inotify_rm_watch(inotify_fd_, wd);
    }
    wd_to_path_.clear();
    close(inotify_fd_);
    inotify_fd_ = -1;
  }

  LOG(INFO) << "SkillWatcher stopped";
}

void SkillWatcher::WatchLoop() {
  constexpr size_t kBufSize = 4096;
  char buf[kBufSize]
      __attribute__((aligned(__alignof__(struct inotify_event))));

  bool pending_reload = false;
  auto last_event_time = std::chrono::steady_clock::now();

  while (running_.load()) {
    struct pollfd pfd;
    pfd.fd = inotify_fd_;
    pfd.events = POLLIN;

    // Poll with 100ms timeout for shutdown check
    int ret = poll(&pfd, 1, 100);

    if (ret > 0 && (pfd.revents & POLLIN)) {
      ssize_t len = read(inotify_fd_, buf, kBufSize);
      if (len <= 0) continue;

      const char* ptr = buf;
      while (ptr < buf + len) {
        auto* event = reinterpret_cast<const struct inotify_event*>(ptr);

        if (event->len > 0) {
          std::string name(event->name);

          // New subdirectory created: add watch
          if ((event->mask & IN_CREATE) && (event->mask & IN_ISDIR)) {
            std::string subdir;
            {
              std::lock_guard<std::mutex> lock(wd_mutex_);
              auto it = wd_to_path_.find(event->wd);
              if (it != wd_to_path_.end()) {
                subdir = it->second + "/" + name;
              }
            }
            if (!subdir.empty()) {
              AddSubdirWatch(subdir);
              LOG(INFO) << "New skill dir: " << name;
            }
          }

          // manifest.json changed
          if (name == kManifestFile) {
            pending_reload = true;
            last_event_time = std::chrono::steady_clock::now();
            LOG(INFO) << "Skill manifest changed: " << name;
          }
        }

        ptr += sizeof(struct inotify_event) + event->len;
      }
    }

    // Debounce: fire callback after no events
    // for kDebounceMs
    if (pending_reload) {
      auto now = std::chrono::steady_clock::now();
      auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                         now - last_event_time)
                         .count();
      if (elapsed >= kDebounceMs) {
        pending_reload = false;
        LOG(INFO) << "Triggering skill reload";
        if (callback_) {
          callback_();
        }
      }
    }
  }
}

void SkillWatcher::AddSubdirWatch(const std::string& path) {
  int wd = inotify_add_watch(inotify_fd_, path.c_str(), kWatchMask);
  if (wd < 0) {
    LOG(WARNING) << "Failed to watch subdir " << path << ": "
                 << strerror(errno);
    return;
  }

  std::lock_guard<std::mutex> lock(wd_mutex_);
  wd_to_path_[wd] = path;
}

void SkillWatcher::ScanSubdirectories() {
  DIR* dir = opendir(skills_dir_.c_str());
  if (!dir) return;

  struct dirent* ent;
  while ((ent = readdir(dir)) != nullptr) {
    if (ent->d_name[0] == '.') continue;

    std::string subpath = skills_dir_ + "/" + ent->d_name;
    struct stat st;
    if (stat(subpath.c_str(), &st) == 0 && S_ISDIR(st.st_mode)) {
      AddSubdirWatch(subpath);
    }
  }
  closedir(dir);
}

}  // namespace tizenclaw
