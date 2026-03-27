#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <atomic>
#include <chrono>
#include <fstream>
#include <thread>

#include "skill_watcher.hh"

using namespace tizenclaw;


class SkillWatcherTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name = ::testing::UnitTest::GetInstance()->current_test_info()->name();
    test_dir_ = std::string("test_skills_watch_") + test_name;
    mkdir(test_dir_.c_str(), 0755);
  }

  void TearDown() override {
    // Cleanup test directories
    int ret = system(("rm -rf " + test_dir_).c_str());
    (void)ret;
  }

  std::string test_dir_;
};

TEST_F(SkillWatcherTest, StartAndStop) {
  SkillWatcher watcher;
  bool callback_called = false;

  EXPECT_TRUE(watcher.Start(
      test_dir_,
      [&]() { callback_called = true; }));
  EXPECT_TRUE(watcher.IsRunning());

  watcher.Stop();
  EXPECT_FALSE(watcher.IsRunning());
}

TEST_F(SkillWatcherTest,
       DoubleStartReturnsFalse) {
  SkillWatcher watcher;

  EXPECT_TRUE(watcher.Start(
      test_dir_, []() {}));
  EXPECT_FALSE(watcher.Start(
      test_dir_, []() {}));

  watcher.Stop();
}

TEST_F(SkillWatcherTest,
       ManifestChangeTriggersCallback) {
  SkillWatcher watcher;
  std::atomic<int> callback_count{0};

  // Create a skill subdirectory
  std::string skill_dir =
      test_dir_ + "/test_skill";
  mkdir(skill_dir.c_str(), 0755);

  EXPECT_TRUE(watcher.Start(
      test_dir_,
      [&]() { callback_count++; }));

  // Create manifest.json in the skill dir
  std::ofstream f(
      skill_dir + "/manifest.json");
  f << R"({"name": "test_skill"})"
    << std::endl;
  f.close();

  // Wait for debounce (500ms) + margin
  std::this_thread::sleep_for(
      std::chrono::milliseconds(800));

  EXPECT_GE(callback_count.load(), 1);

  watcher.Stop();
}

TEST_F(SkillWatcherTest,
       NonManifestFileIgnored) {
  SkillWatcher watcher;
  std::atomic<int> callback_count{0};

  // Create a skill subdirectory
  std::string skill_dir =
      test_dir_ + "/test_skill2";
  mkdir(skill_dir.c_str(), 0755);

  EXPECT_TRUE(watcher.Start(
      test_dir_,
      [&]() { callback_count++; }));

  // Create a non-manifest file
  std::ofstream f(
      skill_dir + "/run.py");
  f << "print('hello')" << std::endl;
  f.close();

  // Wait for debounce period
  std::this_thread::sleep_for(
      std::chrono::milliseconds(800));

  EXPECT_EQ(callback_count.load(), 0);

  watcher.Stop();
}

TEST_F(SkillWatcherTest,
       InvalidDirectoryFails) {
  SkillWatcher watcher;

  EXPECT_FALSE(watcher.Start(
      "/nonexistent/path", []() {}));
  EXPECT_FALSE(watcher.IsRunning());
}

TEST_F(SkillWatcherTest,
       StopWithoutStartIsSafe) {
  SkillWatcher watcher;
  watcher.Stop();  // Should not crash
  EXPECT_FALSE(watcher.IsRunning());
}
