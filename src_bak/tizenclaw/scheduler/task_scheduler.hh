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
#ifndef TASK_SCHEDULER_HH
#define TASK_SCHEDULER_HH

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <functional>
#include <json.hpp>
#include <map>
#include <mutex>
#include <queue>
#include <string>
#include <thread>
#include <vector>

namespace tizenclaw {

class AgentCore;  // forward declaration

// Schedule type enumeration
enum class ScheduleType {
  kOnce,      // "once 2026-03-10 14:00"
  kDaily,     // "daily 09:00"
  kWeekly,    // "weekly mon 09:00"
  kInterval,  // "interval 30m"
};

// Task status
enum class TaskStatus {
  kActive,
  kPaused,
  kCompleted,
  kFailed,
  kCancelled,
};

// Task execution history entry
struct TaskExecEntry {
  std::string timestamp;
  std::string status;  // "success" or "failed"
  int duration_ms = 0;
  std::string result_summary;
};

// Scheduled task definition
struct ScheduledTask {
  std::string id;
  ScheduleType schedule_type;
  std::string schedule_expr;  // raw expression
  std::string prompt;
  std::string session_id;
  TaskStatus status = TaskStatus::kActive;

  // Time tracking
  std::chrono::system_clock::time_point next_run;
  std::chrono::system_clock::time_point created_at;
  std::chrono::system_clock::time_point last_run;

  int run_count = 0;
  int fail_count = 0;
  int max_retries = 3;

  // Interval-specific (seconds)
  int interval_seconds = 0;

  // Daily/Weekly specific
  int hour = 0;
  int minute = 0;
  int weekday = -1;  // 0=Sun..6=Sat, -1=N/A

  // Execution history
  std::vector<TaskExecEntry> history;
};

// Compare tasks by next_run (priority queue)
struct TaskTimeCompare {
  bool operator()(const ScheduledTask* a, const ScheduledTask* b) const {
    return a->next_run > b->next_run;
  }
};

class TaskScheduler {
 public:
  TaskScheduler();
  ~TaskScheduler();

  // Start scheduler threads with AgentCore ref
  void Start(AgentCore* agent);
  // Stop scheduler gracefully
  void Stop();

  // Task CRUD operations
  std::string CreateTask(const std::string& schedule_expr,
                         const std::string& prompt,
                         const std::string& session_id);

  nlohmann::json ListTasks(const std::string& session_id = "");

  bool CancelTask(const std::string& task_id);

  nlohmann::json GetTaskHistory(const std::string& task_id);

 private:
  // Thread entry points
  void SchedulerLoop();
  void ExecutorLoop();

  // Schedule parsing
  bool ParseSchedule(const std::string& expr, ScheduledTask& task);

  // Compute next execution time
  void ComputeNextRun(ScheduledTask& task);

  // Execute a single task
  void ExecuteTask(ScheduledTask& task);

  // Persistence (Markdown)
  void LoadTasks();
  void SaveTask(const ScheduledTask& task);
  void DeleteTaskFile(const std::string& task_id);

  // Generate unique task ID
  static std::string GenerateTaskId();

  // Convert types to/from strings
  static std::string ScheduleTypeToString(ScheduleType type);
  static ScheduleType StringToScheduleType(const std::string& s);
  static std::string TaskStatusToString(TaskStatus status);
  static TaskStatus StringToTaskStatus(const std::string& s);

  // Get tasks directory path
  std::string GetTasksDir() const;

  // Format time_point as ISO string
  static std::string FormatTime(
      const std::chrono::system_clock::time_point& tp);
  // Parse ISO string to time_point
  static std::chrono::system_clock::time_point ParseTime(const std::string& s);

  AgentCore* agent_ = nullptr;

  // All tasks keyed by ID
  std::map<std::string, ScheduledTask> tasks_;
  std::mutex tasks_mutex_;

  // Execution queue
  std::queue<std::string> exec_queue_;
  std::mutex queue_mutex_;
  std::condition_variable queue_cv_;

  // Scheduler timer
  std::thread scheduler_thread_;
  std::thread executor_thread_;
  std::condition_variable scheduler_cv_;
  std::atomic<bool> running_{false};

  // Data directory
  static constexpr const char* kDataDir = "/opt/usr/share/tizenclaw";

  // Max execution history entries per task
  static constexpr size_t kMaxHistoryEntries = 50;
};

}  // namespace tizenclaw

#endif  // TASK_SCHEDULER_HH
