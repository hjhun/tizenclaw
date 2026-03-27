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
#include "task_scheduler.hh"

#include <dirent.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <ctime>
#include <fstream>
#include <iomanip>
#include <random>
#include <sstream>

#include "../../common/logging.hh"
#include "../core/agent_core.hh"

namespace tizenclaw {

TaskScheduler::TaskScheduler() {}

TaskScheduler::~TaskScheduler() { Stop(); }

void TaskScheduler::Start(AgentCore* agent) {
  if (running_.load()) return;
  agent_ = agent;
  running_ = true;

  // Load persisted tasks
  LoadTasks();

  // Start scheduler timer thread
  scheduler_thread_ = std::thread(&TaskScheduler::SchedulerLoop, this);

  // Start executor worker thread
  executor_thread_ = std::thread(&TaskScheduler::ExecutorLoop, this);

  LOG(INFO) << "TaskScheduler started with " << tasks_.size() << " tasks";
}

void TaskScheduler::Stop() {
  if (!running_.load()) return;
  running_ = false;

  // Wake up both threads
  scheduler_cv_.notify_all();
  queue_cv_.notify_all();

  if (scheduler_thread_.joinable()) scheduler_thread_.join();
  if (executor_thread_.joinable()) executor_thread_.join();

  LOG(INFO) << "TaskScheduler stopped";
}

// -------------------------------------------------
// Scheduler Timer Loop
// -------------------------------------------------
void TaskScheduler::SchedulerLoop() {
  LOG(INFO) << "Scheduler timer thread started";

  while (running_.load()) {
    std::unique_lock<std::mutex> lock(tasks_mutex_);

    // Find the earliest next_run
    auto now = std::chrono::system_clock::now();
    std::chrono::system_clock::time_point earliest =
        now + std::chrono::hours(24);
    std::string earliest_id;

    for (auto& [id, task] : tasks_) {
      if (task.status != TaskStatus::kActive) continue;
      if (task.next_run <= now) {
        // Task is due — enqueue immediately
        {
          std::lock_guard<std::mutex> qlock(queue_mutex_);
          exec_queue_.push(id);
        }
        queue_cv_.notify_one();

        // Compute next run for repeating tasks
        if (task.schedule_type == ScheduleType::kOnce) {
          task.status = TaskStatus::kCompleted;
        } else {
          ComputeNextRun(task);
        }
        SaveTask(task);
        continue;
      }
      if (task.next_run < earliest) {
        earliest = task.next_run;
        earliest_id = id;
      }
    }

    // Wait until the earliest task or wakeup
    scheduler_cv_.wait_until(lock, earliest);
  }

  LOG(INFO) << "Scheduler timer thread exiting";
}

// -------------------------------------------------
// Executor Worker Loop
// -------------------------------------------------
void TaskScheduler::ExecutorLoop() {
  LOG(INFO) << "Executor worker thread started";

  while (running_.load()) {
    std::string task_id;
    {
      std::unique_lock<std::mutex> lock(queue_mutex_);
      queue_cv_.wait(
          lock, [this]() { return !exec_queue_.empty() || !running_.load(); });
      if (!running_.load() && exec_queue_.empty()) break;
      if (exec_queue_.empty()) continue;
      task_id = exec_queue_.front();
      exec_queue_.pop();
    }

    ScheduledTask task_copy;
    {
      std::lock_guard<std::mutex> lock(tasks_mutex_);
      auto it = tasks_.find(task_id);
      if (it == tasks_.end()) continue;
      if (it->second.status == TaskStatus::kCancelled) continue;
      task_copy = it->second;
    }

    ExecuteTask(task_copy);
  }

  LOG(INFO) << "Executor worker thread exiting";
}

// -------------------------------------------------
// Execute a single task
// -------------------------------------------------
void TaskScheduler::ExecuteTask(ScheduledTask& task) {
  LOG(INFO) << "Executing task " << task.id << ": " << task.prompt;

  auto start = std::chrono::steady_clock::now();

  TaskExecEntry entry;
  entry.timestamp = FormatTime(std::chrono::system_clock::now());

  // Use a scheduler-specific session_id
  std::string sched_session = "scheduler_" + task.id;

  try {
    std::string result;
    if (agent_) {
      result = agent_->ProcessPrompt(sched_session, task.prompt);
    } else {
      result = "Error: AgentCore not available";
    }

    auto end = std::chrono::steady_clock::now();
    entry.duration_ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(end - start)
            .count();

    // Truncate long results for history
    if (result.size() > 200) {
      entry.result_summary = result.substr(0, 197) + "...";
    } else {
      entry.result_summary = result;
    }

    entry.status = "success";
    task.fail_count = 0;

    LOG(INFO) << "Task " << task.id << " completed in " << entry.duration_ms
              << "ms";
  } catch (const std::exception& e) {
    auto end = std::chrono::steady_clock::now();
    entry.duration_ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(end - start)
            .count();
    entry.status = "failed";
    entry.result_summary = e.what();
    task.fail_count++;

    LOG(ERROR) << "Task " << task.id << " failed: " << e.what();

    // Disable after max retries
    if (task.fail_count >= task.max_retries) {
      task.status = TaskStatus::kFailed;
      LOG(WARNING) << "Task " << task.id << " disabled after "
                   << task.max_retries << " failures";
    }
  }

  task.last_run = std::chrono::system_clock::now();
  task.run_count++;

  // Add to history (trim if too many)
  task.history.insert(task.history.begin(), entry);
  if (task.history.size() > kMaxHistoryEntries) {
    task.history.resize(kMaxHistoryEntries);
  }

  {
    std::lock_guard<std::mutex> lock(tasks_mutex_);
    if (tasks_.contains(task.id)) {
      tasks_[task.id] = task;
      SaveTask(task);
    }
  }
}

// -------------------------------------------------
// Task CRUD
// -------------------------------------------------
std::string TaskScheduler::CreateTask(const std::string& schedule_expr,
                                      const std::string& prompt,
                                      const std::string& session_id) {
  ScheduledTask task;
  task.id = GenerateTaskId();
  task.schedule_expr = schedule_expr;
  task.prompt = prompt;
  task.session_id = session_id;
  task.status = TaskStatus::kActive;
  task.created_at = std::chrono::system_clock::now();

  if (!ParseSchedule(schedule_expr, task)) {
    LOG(ERROR) << "Invalid schedule: " << schedule_expr;
    return "";
  }

  ComputeNextRun(task);

  std::lock_guard<std::mutex> lock(tasks_mutex_);
  tasks_[task.id] = task;
  SaveTask(task);

  // Wake scheduler to recalculate timer
  scheduler_cv_.notify_all();

  LOG(INFO) << "Created task " << task.id << " schedule=" << schedule_expr
            << " next_run=" << FormatTime(task.next_run);

  return task.id;
}

nlohmann::json TaskScheduler::ListTasks(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(tasks_mutex_);

  nlohmann::json result = nlohmann::json::array();

  for (auto& [id, task] : tasks_) {
    if (!session_id.empty() && task.session_id != session_id) continue;

    nlohmann::json j;
    j["id"] = task.id;
    j["schedule"] = task.schedule_expr;
    j["prompt"] = task.prompt;
    j["session_id"] = task.session_id;
    j["status"] = TaskStatusToString(task.status);
    j["next_run"] = FormatTime(task.next_run);
    j["run_count"] = task.run_count;
    j["created_at"] = FormatTime(task.created_at);
    if (task.run_count > 0) {
      j["last_run"] = FormatTime(task.last_run);
    }
    result.push_back(j);
  }

  return result;
}

bool TaskScheduler::CancelTask(const std::string& task_id) {
  std::lock_guard<std::mutex> lock(tasks_mutex_);

  auto it = tasks_.find(task_id);
  if (it == tasks_.end()) return false;

  it->second.status = TaskStatus::kCancelled;
  SaveTask(it->second);

  LOG(INFO) << "Cancelled task " << task_id;
  return true;
}

nlohmann::json TaskScheduler::GetTaskHistory(const std::string& task_id) {
  std::lock_guard<std::mutex> lock(tasks_mutex_);

  auto it = tasks_.find(task_id);
  if (it == tasks_.end()) {
    return {{"error", "Task not found"}};
  }

  auto& task = it->second;
  nlohmann::json result;
  result["id"] = task.id;
  result["schedule"] = task.schedule_expr;
  result["status"] = TaskStatusToString(task.status);
  result["run_count"] = task.run_count;

  nlohmann::json hist = nlohmann::json::array();
  for (auto& e : task.history) {
    hist.push_back({
        {"timestamp", e.timestamp},
        {"status", e.status},
        {"duration_ms", e.duration_ms},
        {"result", e.result_summary},
    });
  }
  result["history"] = hist;
  return result;
}

// -------------------------------------------------
// Schedule Parsing
// -------------------------------------------------
bool TaskScheduler::ParseSchedule(const std::string& expr,
                                  ScheduledTask& task) {
  std::istringstream iss(expr);
  std::string type_str;
  iss >> type_str;

  // Normalize to lowercase
  std::transform(type_str.begin(), type_str.end(), type_str.begin(), ::tolower);

  if (type_str == "daily") {
    // Format: "daily HH:MM"
    std::string time_str;
    iss >> time_str;
    auto pos = time_str.find(':');
    if (pos == std::string::npos) return false;

    try {
      task.hour = std::stoi(time_str.substr(0, pos));
      task.minute = std::stoi(time_str.substr(pos + 1));
    } catch (...) {
      return false;
    }

    if (task.hour < 0 || task.hour > 23 || task.minute < 0 || task.minute > 59)
      return false;

    task.schedule_type = ScheduleType::kDaily;
    return true;

  } else if (type_str == "interval") {
    // Format: "interval Ns" / "Nm" / "Nh"
    std::string val_str;
    iss >> val_str;
    if (val_str.empty()) return false;

    char unit = val_str.back();
    int val = 0;
    try {
      val = std::stoi(val_str.substr(0, val_str.size() - 1));
    } catch (...) {
      return false;
    }
    if (val <= 0) return false;

    switch (unit) {
      case 's':
        task.interval_seconds = val;
        break;
      case 'm':
        task.interval_seconds = val * 60;
        break;
      case 'h':
        task.interval_seconds = val * 3600;
        break;
      default:
        return false;
    }

    task.schedule_type = ScheduleType::kInterval;
    return true;

  } else if (type_str == "once") {
    // Format: "once YYYY-MM-DD HH:MM"
    std::string date_str, time_str;
    iss >> date_str >> time_str;
    std::string datetime = date_str + " " + time_str;

    struct tm tm_val = {};
    if (strptime(datetime.c_str(), "%Y-%m-%d %H:%M", &tm_val) == nullptr) {
      return false;
    }
    tm_val.tm_isdst = -1;
    time_t t = mktime(&tm_val);
    if (t == -1) return false;

    task.next_run = std::chrono::system_clock::from_time_t(t);
    task.schedule_type = ScheduleType::kOnce;
    return true;

  } else if (type_str == "weekly") {
    // Format: "weekly DAY HH:MM"
    std::string day_str, time_str;
    iss >> day_str >> time_str;

    std::transform(day_str.begin(), day_str.end(), day_str.begin(), ::tolower);

    // Parse weekday
    static const std::map<std::string, int> day_map = {
        {"sun", 0}, {"mon", 1}, {"tue", 2}, {"wed", 3},
        {"thu", 4}, {"fri", 5}, {"sat", 6},
    };

    auto dit = day_map.find(day_str);
    if (dit == day_map.end()) return false;
    task.weekday = dit->second;

    auto pos = time_str.find(':');
    if (pos == std::string::npos) return false;

    try {
      task.hour = std::stoi(time_str.substr(0, pos));
      task.minute = std::stoi(time_str.substr(pos + 1));
    } catch (...) {
      return false;
    }

    if (task.hour < 0 || task.hour > 23 || task.minute < 0 || task.minute > 59)
      return false;

    task.schedule_type = ScheduleType::kWeekly;
    return true;
  }

  return false;
}

// -------------------------------------------------
// Compute Next Run Time
// -------------------------------------------------
void TaskScheduler::ComputeNextRun(ScheduledTask& task) {
  auto now = std::chrono::system_clock::now();
  time_t now_t = std::chrono::system_clock::to_time_t(now);
  struct tm now_tm;
  localtime_r(&now_t, &now_tm);

  switch (task.schedule_type) {
    case ScheduleType::kDaily: {
      struct tm next = now_tm;
      next.tm_hour = task.hour;
      next.tm_min = task.minute;
      next.tm_sec = 0;
      next.tm_isdst = -1;
      time_t next_t = mktime(&next);
      if (next_t <= now_t) {
        // Already past today, schedule tomorrow
        next_t += 24 * 3600;
      }
      task.next_run = std::chrono::system_clock::from_time_t(next_t);
      break;
    }

    case ScheduleType::kInterval: {
      task.next_run = now + std::chrono::seconds(task.interval_seconds);
      break;
    }

    case ScheduleType::kOnce: {
      // next_run already set during parsing
      break;
    }

    case ScheduleType::kWeekly: {
      struct tm next = now_tm;
      next.tm_hour = task.hour;
      next.tm_min = task.minute;
      next.tm_sec = 0;
      next.tm_isdst = -1;

      // Calculate days until target weekday
      int days_ahead = task.weekday - now_tm.tm_wday;
      if (days_ahead < 0) days_ahead += 7;
      if (days_ahead == 0) {
        // Same day — check if time already past
        time_t candidate = mktime(&next);
        if (candidate <= now_t) {
          days_ahead = 7;
        }
      }
      next.tm_mday += days_ahead;
      next.tm_isdst = -1;
      time_t next_t = mktime(&next);
      task.next_run = std::chrono::system_clock::from_time_t(next_t);
      break;
    }
  }
}

// -------------------------------------------------
// Persistence: Markdown
// -------------------------------------------------
std::string TaskScheduler::GetTasksDir() const {
  return std::string(kDataDir) + "/tasks";
}

void TaskScheduler::LoadTasks() {
  std::string dir = GetTasksDir();

  DIR* d = opendir(dir.c_str());
  if (!d) {
    LOG(INFO) << "No tasks directory, " << "starting fresh";
    return;
  }

  struct dirent* entry;
  while ((entry = readdir(d)) != nullptr) {
    std::string name = entry->d_name;
    if (name.size() < 4 || name.substr(name.size() - 3) != ".md") continue;

    std::string path = dir + "/" + name;
    std::ifstream f(path);
    if (!f.is_open()) continue;

    std::string content((std::istreambuf_iterator<char>(f)),
                        std::istreambuf_iterator<char>());
    f.close();

    // Parse YAML frontmatter
    if (content.substr(0, 4) != "---\n") continue;
    auto end_pos = content.find("\n---\n", 4);
    if (end_pos == std::string::npos) continue;

    std::string frontmatter = content.substr(4, end_pos - 4);

    ScheduledTask task;

    // Simple YAML key-value parser
    std::istringstream yaml(frontmatter);
    std::string line;
    while (std::getline(yaml, line)) {
      auto colon = line.find(": ");
      if (colon == std::string::npos) continue;
      std::string key = line.substr(0, colon);
      std::string val = line.substr(colon + 2);

      // Remove surrounding quotes
      if (val.size() >= 2 && val.front() == '"' && val.back() == '"') {
        val = val.substr(1, val.size() - 2);
      }

      if (key == "id") {
        task.id = val;
      } else if (key == "schedule") {
        task.schedule_expr = val;
      } else if (key == "prompt") {
        task.prompt = val;
      } else if (key == "session_id") {
        task.session_id = val;
      } else if (key == "status") {
        task.status = StringToTaskStatus(val);
      } else if (key == "created_at") {
        task.created_at = ParseTime(val);
      } else if (key == "next_run") {
        task.next_run = ParseTime(val);
      } else if (key == "last_run") {
        task.last_run = ParseTime(val);
      } else if (key == "run_count") {
        try {
          task.run_count = std::stoi(val);
        } catch (...) {
        }
      } else if (key == "fail_count") {
        try {
          task.fail_count = std::stoi(val);
        } catch (...) {
        }
      } else if (key == "max_retries") {
        try {
          task.max_retries = std::stoi(val);
        } catch (...) {
        }
      }
    }

    if (task.id.empty()) continue;

    // Re-parse the schedule expression
    ParseSchedule(task.schedule_expr, task);

    // Parse execution history table
    auto hist_pos = content.find("## Execution History");
    if (hist_pos != std::string::npos) {
      std::istringstream hist_stream(content.substr(hist_pos));
      std::string hline;
      // Skip header lines (title, table header,
      // separator)
      int skip = 0;
      while (std::getline(hist_stream, hline)) {
        if (hline.empty()) continue;
        if (hline[0] == '#') {
          skip = 0;
          continue;
        }
        if (hline[0] == '|') {
          skip++;
          if (skip <= 2) continue;

          // Parse table row:
          // | # | Timestamp | Status | Dur | Result |
          std::vector<std::string> cols;
          std::istringstream row(hline);
          std::string cell;
          while (std::getline(row, cell, '|')) {
            // Trim
            auto s = cell.find_first_not_of(' ');
            auto e = cell.find_last_not_of(' ');
            if (s != std::string::npos && e != std::string::npos) {
              cols.push_back(cell.substr(s, e - s + 1));
            }
          }

          if (cols.size() >= 4) {
            TaskExecEntry te;
            te.timestamp = cols[1];
            te.status = cols[2];
            try {
              // Remove "ms" suffix
              std::string dur = cols[3];
              auto ms_pos = dur.find("ms");
              if (ms_pos != std::string::npos) dur = dur.substr(0, ms_pos);
              te.duration_ms = std::stoi(dur);
            } catch (...) {
            }
            if (cols.size() >= 5) {
              te.result_summary = cols[4];
            }
            task.history.push_back(te);
          }
        }
      }
    }

    // Only reload active/paused tasks
    if (task.status == TaskStatus::kActive ||
        task.status == TaskStatus::kPaused) {
      // Recompute next_run for recurring tasks
      if (task.schedule_type != ScheduleType::kOnce) {
        auto now = std::chrono::system_clock::now();
        if (task.next_run <= now) {
          ComputeNextRun(task);
        }
      }
      tasks_[task.id] = task;
      LOG(INFO) << "Loaded task " << task.id
                << " next_run=" << FormatTime(task.next_run);
    }
  }

  closedir(d);
}

void TaskScheduler::SaveTask(const ScheduledTask& task) {
  std::string dir = GetTasksDir();

  // Ensure directory exists
  mkdir(dir.c_str(), 0755);

  std::string path = dir + "/task-" + task.id + ".md";

  std::ostringstream out;
  out << "---\n";
  out << "id: " << task.id << "\n";
  out << "schedule: " << task.schedule_expr << "\n";
  out << "prompt: \"" << task.prompt << "\"\n";
  out << "session_id: " << task.session_id << "\n";
  out << "status: " << TaskStatusToString(task.status) << "\n";
  out << "created_at: " << FormatTime(task.created_at) << "\n";
  out << "next_run: " << FormatTime(task.next_run) << "\n";
  if (task.run_count > 0) {
    out << "last_run: " << FormatTime(task.last_run) << "\n";
  }
  out << "run_count: " << task.run_count << "\n";
  out << "fail_count: " << task.fail_count << "\n";
  out << "max_retries: " << task.max_retries << "\n";
  out << "---\n\n";

  // Execution history table
  if (!task.history.empty()) {
    out << "## Execution History\n\n";
    out << "| # | Timestamp | Status" << " | Duration | Result |\n";
    out << "|---|-----------|--------" << "|----------|--------|\n";

    int num = task.run_count;
    for (auto& e : task.history) {
      out << "| " << num-- << " | " << e.timestamp << " | " << e.status << " | "
          << e.duration_ms << "ms | " << e.result_summary << " |\n";
    }
  }

  // Atomic write: write .tmp then rename
  std::string tmp_path = path + ".tmp";
  std::ofstream f(tmp_path);
  if (f.is_open()) {
    f << out.str();
    f.close();
    rename(tmp_path.c_str(), path.c_str());
  }
}

void TaskScheduler::DeleteTaskFile(const std::string& task_id) {
  std::string path = GetTasksDir() + "/task-" + task_id + ".md";
  unlink(path.c_str());
}

// -------------------------------------------------
// Utility functions
// -------------------------------------------------
std::string TaskScheduler::GenerateTaskId() {
  auto now = std::chrono::system_clock::now();
  auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(
                now.time_since_epoch())
                .count();

  std::random_device rd;
  std::mt19937 gen(rd());
  std::uniform_int_distribution<int> dist(0x1000, 0xFFFF);

  std::ostringstream oss;
  oss << std::hex << (ms & 0xFFFFFF) << "-" << dist(gen);
  return oss.str();
}

std::string TaskScheduler::FormatTime(
    const std::chrono::system_clock::time_point& tp) {
  time_t t = std::chrono::system_clock::to_time_t(tp);
  struct tm tm_val;
  localtime_r(&t, &tm_val);

  char buf[32];
  strftime(buf, sizeof(buf), "%Y-%m-%dT%H:%M:%S", &tm_val);
  return buf;
}

std::chrono::system_clock::time_point TaskScheduler::ParseTime(
    const std::string& s) {
  struct tm tm_val = {};
  if (strptime(s.c_str(), "%Y-%m-%dT%H:%M:%S", &tm_val) != nullptr) {
    tm_val.tm_isdst = -1;
    time_t t = mktime(&tm_val);
    return std::chrono::system_clock::from_time_t(t);
  }
  // Fallback for date-only
  if (strptime(s.c_str(), "%Y-%m-%d %H:%M", &tm_val) != nullptr) {
    tm_val.tm_isdst = -1;
    time_t t = mktime(&tm_val);
    return std::chrono::system_clock::from_time_t(t);
  }
  return std::chrono::system_clock::now();
}

std::string TaskScheduler::ScheduleTypeToString(ScheduleType type) {
  switch (type) {
    case ScheduleType::kOnce:
      return "once";
    case ScheduleType::kDaily:
      return "daily";
    case ScheduleType::kWeekly:
      return "weekly";
    case ScheduleType::kInterval:
      return "interval";
  }
  return "unknown";
}

ScheduleType TaskScheduler::StringToScheduleType(const std::string& s) {
  if (s == "once") return ScheduleType::kOnce;
  if (s == "daily") return ScheduleType::kDaily;
  if (s == "weekly") return ScheduleType::kWeekly;
  if (s == "interval") return ScheduleType::kInterval;
  return ScheduleType::kOnce;
}

std::string TaskScheduler::TaskStatusToString(TaskStatus status) {
  switch (status) {
    case TaskStatus::kActive:
      return "active";
    case TaskStatus::kPaused:
      return "paused";
    case TaskStatus::kCompleted:
      return "completed";
    case TaskStatus::kFailed:
      return "failed";
    case TaskStatus::kCancelled:
      return "cancelled";
  }
  return "unknown";
}

TaskStatus TaskScheduler::StringToTaskStatus(const std::string& s) {
  if (s == "active") return TaskStatus::kActive;
  if (s == "paused") return TaskStatus::kPaused;
  if (s == "completed") return TaskStatus::kCompleted;
  if (s == "failed") return TaskStatus::kFailed;
  if (s == "cancelled") return TaskStatus::kCancelled;
  return TaskStatus::kActive;
}

}  // namespace tizenclaw
