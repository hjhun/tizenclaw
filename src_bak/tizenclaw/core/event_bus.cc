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
#include "event_bus.hh"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

EventBus& EventBus::GetInstance() {
  static EventBus instance;
  return instance;
}

EventBus::EventBus() = default;

EventBus::~EventBus() {
  Stop();
}

void EventBus::Start() {
  if (running_.load()) return;

  running_.store(true);
  dispatch_thread_ = std::thread(&EventBus::DispatchLoop, this);
  LOG(INFO) << "EventBus started";
}

void EventBus::Stop() {
  if (!running_.load()) return;

  running_.store(false);
  queue_cv_.notify_all();
  if (dispatch_thread_.joinable()) {
    dispatch_thread_.join();
  }
  LOG(INFO) << "EventBus stopped";
}

void EventBus::Publish(SystemEvent event) {
  // Set timestamp if not set
  if (event.timestamp == 0) {
    event.timestamp =
        std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::system_clock::now().time_since_epoch())
            .count();
  }

  {
    std::lock_guard<std::mutex> lock(queue_mutex_);

    // Bounded queue: drop oldest if full
    if (queue_.size() >= kMaxQueueSize) {
      queue_.pop_front();
    }
    queue_.push_back(std::move(event));
  }

  queue_cv_.notify_one();
}

int EventBus::Subscribe(EventType type, EventCallback callback) {
  std::lock_guard<std::mutex> lock(sub_mutex_);
  int id = next_sub_id_++;
  subscribers_.push_back({id, type, false, std::move(callback)});
  return id;
}

int EventBus::SubscribeAll(EventCallback callback) {
  std::lock_guard<std::mutex> lock(sub_mutex_);
  int id = next_sub_id_++;
  subscribers_.push_back({id, EventType::kCustom, true, std::move(callback)});
  return id;
}

void EventBus::Unsubscribe(int subscription_id) {
  std::lock_guard<std::mutex> lock(sub_mutex_);
  std::erase_if(subscribers_, [subscription_id](const Subscription& s) {
    return s.id == subscription_id;
  });
}

void EventBus::RegisterEventSource(const EventSourceDescriptor& desc) {
  std::lock_guard<std::mutex> lock(sources_mutex_);

  // Remove existing with same name
  std::erase_if(event_sources_, [&desc](const EventSourceDescriptor& d) {
    return d.name == desc.name;
  });

  event_sources_.push_back(desc);
  LOG(INFO) << "EventBus: registered source '"
            << desc.name << "' (" << desc.plugin_id << ")";
}

void EventBus::UnregisterEventSource(const std::string& source_name) {
  std::lock_guard<std::mutex> lock(sources_mutex_);
  std::erase_if(event_sources_, [&source_name](const EventSourceDescriptor& d) {
    return d.name == source_name;
  });
  LOG(INFO) << "EventBus: unregistered source '" << source_name << "'";
}

std::vector<EventSourceDescriptor> EventBus::ListEventSources() const {
  std::lock_guard<std::mutex> lock(sources_mutex_);
  return event_sources_;
}

void EventBus::LoadPlugins(const std::string& events_dir) {
  namespace fs = std::filesystem;
  std::error_code ec;

  if (!fs::is_directory(events_dir, ec)) {
    LOG(WARNING) << "EventBus: events directory "
                 << "not found: " << events_dir;
    return;
  }

  int count = 0;
  for (const auto& entry : fs::directory_iterator(events_dir, ec)) {
    if (!entry.is_directory()) continue;

    std::string md_path =
        entry.path().string() + "/event_source.md";
    std::ifstream f(md_path);
    if (!f.is_open()) continue;

    // Parse YAML frontmatter from MD file
    // (simple parser: lines between --- markers)
    std::string line;
    bool in_frontmatter = false;
    std::string yaml_content;

    while (std::getline(f, line)) {
      if (line == "---") {
        if (!in_frontmatter) {
          in_frontmatter = true;
          continue;
        } else {
          break;  // End of frontmatter
        }
      }
      if (in_frontmatter) {
        yaml_content += line + "\n";
      }
    }
    f.close();

    if (yaml_content.empty()) continue;

    // Simple YAML key-value parsing
    EventSourceDescriptor desc;
    desc.plugin_id = entry.path().filename().string();

    std::istringstream ss(yaml_content);
    std::string key, value;
    while (std::getline(ss, line)) {
      auto colon = line.find(':');
      if (colon == std::string::npos) continue;
      key = line.substr(0, colon);
      value = line.substr(colon + 1);

      // Trim whitespace
      while (!key.empty() && key.front() == ' ') key.erase(key.begin());
      while (!key.empty() && key.back() == ' ') key.pop_back();
      while (!value.empty() && value.front() == ' ') value.erase(value.begin());
      while (!value.empty() && value.back() == ' ') value.pop_back();

      if (key == "name") desc.name = value;
      else if (key == "type") desc.type = value;
      else if (key == "version") desc.version = value;
      else if (key == "collect_method") desc.collect_method = value;
      else if (key == "poll_interval_sec") {
        try {
          desc.poll_interval_sec = std::stoi(value);
        } catch (...) {}
      }
    }

    if (!desc.name.empty()) {
      RegisterEventSource(desc);
      ++count;
    }
  }

  LOG(INFO) << "EventBus: loaded " << count
            << " plugin descriptors from "
            << events_dir;
}

void EventBus::DispatchLoop() {
  while (running_.load()) {
    SystemEvent event;
    {
      std::unique_lock<std::mutex> lock(queue_mutex_);
      queue_cv_.wait(lock, [this] {
        return !queue_.empty() || !running_.load();
      });

      if (!running_.load() && queue_.empty()) break;
      if (queue_.empty()) continue;

      event = std::move(queue_.front());
      queue_.pop_front();
    }

    // Dispatch to subscribers
    std::vector<Subscription> subs_copy;
    {
      std::lock_guard<std::mutex> lock(sub_mutex_);
      subs_copy = subscribers_;
    }

    for (const auto& sub : subs_copy) {
      if (sub.match_all || sub.type == event.type) {
        try {
          sub.callback(event);
        } catch (const std::exception& e) {
          LOG(WARNING) << "EventBus: subscriber "
                       << sub.id << " threw: "
                       << e.what();
        }
      }
    }
  }
}

}  // namespace tizenclaw
