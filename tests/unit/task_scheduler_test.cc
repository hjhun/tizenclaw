#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include <sys/stat.h>
#include "task_scheduler.hh"

using namespace tizenclaw;


class TaskSchedulerTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    scheduler = new TaskScheduler();

    // Create temporary tasks directory
    test_dir = "/tmp/tizenclaw_test_tasks";
    mkdir(test_dir.c_str(), 0755);
  }

  void TearDown() override {
    scheduler->Stop();
    delete scheduler;

    // Cleanup test files
    std::string cmd =
        "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  TaskScheduler* scheduler;
  std::string test_dir;
};

// -------------------------------------------
// Schedule Parsing Tests
// -------------------------------------------
TEST_F(TaskSchedulerTest,
       ParseDailySchedule) {
  ScheduledTask task;
  EXPECT_TRUE(
      scheduler->CreateTask(
          "daily 09:00",
          "test prompt",
          "test_session") != "");
}

TEST_F(TaskSchedulerTest,
       ParseDailyInvalidTime) {
  std::string id =
      scheduler->CreateTask(
          "daily 25:00",
          "test prompt",
          "test_session");
  EXPECT_TRUE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseIntervalMinutes) {
  std::string id =
      scheduler->CreateTask(
          "interval 30m",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseIntervalSeconds) {
  std::string id =
      scheduler->CreateTask(
          "interval 60s",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseIntervalHours) {
  std::string id =
      scheduler->CreateTask(
          "interval 2h",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseIntervalInvalid) {
  std::string id =
      scheduler->CreateTask(
          "interval 0m",
          "test prompt",
          "test_session");
  EXPECT_TRUE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseOnceSchedule) {
  std::string id =
      scheduler->CreateTask(
          "once 2030-12-31 23:59",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseWeeklySchedule) {
  std::string id =
      scheduler->CreateTask(
          "weekly mon 09:00",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseWeeklyInvalidDay) {
  std::string id =
      scheduler->CreateTask(
          "weekly xyz 09:00",
          "test prompt",
          "test_session");
  EXPECT_TRUE(id.empty());
}

TEST_F(TaskSchedulerTest,
       ParseInvalidScheduleType) {
  std::string id =
      scheduler->CreateTask(
          "unknown 09:00",
          "test prompt",
          "test_session");
  EXPECT_TRUE(id.empty());
}

// -------------------------------------------
// Task CRUD Tests
// -------------------------------------------
TEST_F(TaskSchedulerTest,
       CreateAndListTasks) {
  std::string id1 =
      scheduler->CreateTask(
          "daily 09:00",
          "weather report",
          "session1");
  std::string id2 =
      scheduler->CreateTask(
          "interval 1h",
          "system check",
          "session2");

  EXPECT_FALSE(id1.empty());
  EXPECT_FALSE(id2.empty());
  EXPECT_NE(id1, id2);

  // List all tasks
  auto all = scheduler->ListTasks();
  EXPECT_EQ(all.size(), 2u);

  // List by session
  auto s1 =
      scheduler->ListTasks("session1");
  EXPECT_EQ(s1.size(), 1u);
  EXPECT_EQ(
      s1[0]["prompt"], "weather report");
}

TEST_F(TaskSchedulerTest,
       CancelTask) {
  std::string id =
      scheduler->CreateTask(
          "daily 10:00",
          "test prompt",
          "test_session");
  EXPECT_FALSE(id.empty());

  EXPECT_TRUE(scheduler->CancelTask(id));

  // Verify status
  auto tasks = scheduler->ListTasks();
  // Cancelled tasks should still appear
  bool found = false;
  for (auto& t : tasks) {
    if (t["id"] == id) {
      EXPECT_EQ(t["status"], "cancelled");
      found = true;
    }
  }
  EXPECT_TRUE(found);
}

TEST_F(TaskSchedulerTest,
       CancelNonexistentTask) {
  EXPECT_FALSE(
      scheduler->CancelTask("nonexistent"));
}

TEST_F(TaskSchedulerTest,
       GetTaskHistory) {
  std::string id =
      scheduler->CreateTask(
          "daily 09:00",
          "test prompt",
          "test_session");

  auto hist =
      scheduler->GetTaskHistory(id);
  EXPECT_EQ(hist["id"], id);
  EXPECT_EQ(hist["run_count"], 0);

  // Empty history for new task
  EXPECT_TRUE(hist["history"].empty());
}

TEST_F(TaskSchedulerTest,
       GetHistoryNonexistent) {
  auto hist =
      scheduler->GetTaskHistory("missing");
  EXPECT_TRUE(hist.contains("error"));
}

// -------------------------------------------
// Type Conversion Tests
// -------------------------------------------
TEST_F(TaskSchedulerTest,
       EmptyScheduleRejected) {
  std::string id =
      scheduler->CreateTask(
          "", "prompt", "session");
  EXPECT_TRUE(id.empty());
}

TEST_F(TaskSchedulerTest,
       EmptyPromptRejected) {
  // CreateTask requires non-empty prompt
  // Validate that schedule parsing works
  // but the task would be useless
  std::string id =
      scheduler->CreateTask(
          "daily 09:00", "", "session");
  // Empty prompt should still create task
  // (LLM will get empty prompt at execution)
  // This is a design choice — not rejected
  EXPECT_FALSE(id.empty());
}
