#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>

#include "skill_plugin_manager.hh"

using namespace tizenclaw;

class SkillPluginManagerTest : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    namespace fs = std::filesystem;
    test_dir_ = fs::absolute(
        std::string("/tmp/test_skill_plugin_") + test_name).string();
    fs::create_directories(test_dir_ + "/lib/skill_a");
    fs::create_directories(test_dir_ + "/lib/skill_b");
    target_dir_ = fs::absolute(
        std::string("/tmp/test_skill_target_") + test_name).string();
    fs::create_directories(target_dir_);
  }

  void TearDown() override {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::remove_all(test_dir_, ec);
    fs::remove_all(target_dir_, ec);
  }

  void WriteManifest(const std::string& dir,
                     const std::string& name) {
    std::ofstream f(dir + "/manifest.json");
    f << R"({"name": ")" << name
      << R"(", "description": "Test skill", )"
      << R"("parameters": {"type": "object", )"
      << R"("properties": {}, "required": []}, )"
      << R"("entry_point": "skill.py"})";
    f.close();

    std::ofstream py(dir + "/skill.py");
    py << "import json\nprint(json.dumps("
       << "{\"result\": \"ok\"}))\n";
    py.close();
  }

  std::string test_dir_;
  std::string target_dir_;
};

TEST_F(SkillPluginManagerTest,
       ParseSingleSkillName) {
  auto names =
      SkillPluginManager::ParseSkillNames("get_wifi_status");
  ASSERT_EQ(names.size(), 1u);
  EXPECT_EQ(names[0], "get_wifi_status");
}

TEST_F(SkillPluginManagerTest,
       ParsePipeDelimitedSkillNames) {
  auto names =
      SkillPluginManager::ParseSkillNames(
          "skill_a|skill_b|skill_c");
  ASSERT_EQ(names.size(), 3u);
  EXPECT_EQ(names[0], "skill_a");
  EXPECT_EQ(names[1], "skill_b");
  EXPECT_EQ(names[2], "skill_c");
}

TEST_F(SkillPluginManagerTest,
       ParsePipeDelimitedWithSpaces) {
  auto names =
      SkillPluginManager::ParseSkillNames(
          "skill_a | skill_b | skill_c");
  ASSERT_EQ(names.size(), 3u);
  EXPECT_EQ(names[0], "skill_a");
  EXPECT_EQ(names[1], "skill_b");
  EXPECT_EQ(names[2], "skill_c");
}

TEST_F(SkillPluginManagerTest,
       ParseEmptyValue) {
  auto names =
      SkillPluginManager::ParseSkillNames("");
  EXPECT_TRUE(names.empty());
}

TEST_F(SkillPluginManagerTest,
       ParseTrailingPipe) {
  auto names =
      SkillPluginManager::ParseSkillNames("skill_a|");
  ASSERT_EQ(names.size(), 1u);
  EXPECT_EQ(names[0], "skill_a");
}

TEST_F(SkillPluginManagerTest,
       LinkSkillDirCreatesSymlink) {
  namespace fs = std::filesystem;

  std::string source = test_dir_ + "/lib/skill_a";
  WriteManifest(source, "skill_a");

  std::string target = target_dir_ + "/pkg__skill_a";

  SkillPluginManager& mgr = SkillPluginManager::GetInstance();
  EXPECT_TRUE(mgr.LinkSkillDir(source, target));

  // Verify target exists and is a symlink or directory
  EXPECT_TRUE(fs::exists(target));

  // Verify manifest.json is accessible
  EXPECT_TRUE(fs::exists(target + "/manifest.json"));
  EXPECT_TRUE(fs::exists(target + "/skill.py"));
}

TEST_F(SkillPluginManagerTest,
       RemoveSkillDirCleansUp) {
  namespace fs = std::filesystem;

  std::string source = test_dir_ + "/lib/skill_b";
  WriteManifest(source, "skill_b");

  std::string target = target_dir_ + "/pkg__skill_b";

  SkillPluginManager& mgr = SkillPluginManager::GetInstance();
  EXPECT_TRUE(mgr.LinkSkillDir(source, target));
  EXPECT_TRUE(fs::exists(target));

  mgr.RemoveSkillDir(target);
  EXPECT_FALSE(fs::exists(target));
}

TEST_F(SkillPluginManagerTest,
       LinkOverwritesExistingTarget) {
  namespace fs = std::filesystem;

  std::string source = test_dir_ + "/lib/skill_a";
  WriteManifest(source, "skill_a");

  std::string target = target_dir_ + "/pkg__skill_a";
  fs::create_directories(target);

  // Write a dummy file to verify it gets replaced
  {
    std::ofstream f(target + "/old_file.txt");
    f << "old content";
  }

  SkillPluginManager& mgr = SkillPluginManager::GetInstance();
  EXPECT_TRUE(mgr.LinkSkillDir(source, target));

  // Old file should not exist; manifest.json should
  EXPECT_TRUE(fs::exists(target + "/manifest.json"));
}
