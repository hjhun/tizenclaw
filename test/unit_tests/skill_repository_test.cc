#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>
#include "skill_repository.hh"

using namespace tizenclaw;

class SkillRepositoryTest : public ::testing::Test {
 protected:
  void SetUp() override {
    config_path_ =
        std::string("test_skill_repo_") +
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name() +
        ".json";
  }

  void TearDown() override {
    unlink(config_path_.c_str());
  }

  std::string config_path_;
};

TEST_F(SkillRepositoryTest, DisabledByDefault) {
  SkillRepository repo;
  // No config file → disabled
  EXPECT_TRUE(repo.Initialize("/nonexistent.json"));
  EXPECT_FALSE(repo.IsEnabled());
}

TEST_F(SkillRepositoryTest, EnabledViaConfig) {
  std::ofstream f(config_path_);
  f << R"({
    "enabled": true,
    "repository_url": "https://test.example.com/api"
  })" << std::endl;
  f.close();

  SkillRepository repo;
  EXPECT_TRUE(repo.Initialize(config_path_));
  EXPECT_TRUE(repo.IsEnabled());
}

TEST_F(SkillRepositoryTest,
       DisabledInConfig) {
  std::ofstream f(config_path_);
  f << R"({"enabled": false})" << std::endl;
  f.close();

  SkillRepository repo;
  EXPECT_TRUE(repo.Initialize(config_path_));
  EXPECT_FALSE(repo.IsEnabled());
}

TEST_F(SkillRepositoryTest,
       SearchSkillsWhenDisabled) {
  SkillRepository repo;
  repo.Initialize("/nonexistent.json");
  auto results = repo.SearchSkills("bluetooth");
  EXPECT_TRUE(results.empty());
}

TEST_F(SkillRepositoryTest,
       InstallSkillWhenDisabled) {
  SkillRepository repo;
  repo.Initialize("/nonexistent.json");
  auto result = repo.InstallSkill("test_skill");
  EXPECT_FALSE(result.success);
}

TEST_F(SkillRepositoryTest,
       CheckUpdatesWhenDisabled) {
  SkillRepository repo;
  repo.Initialize("/nonexistent.json");
  auto updates = repo.CheckUpdates();
  EXPECT_TRUE(updates.empty());
}

TEST_F(SkillRepositoryTest,
       UninstallNonExistentSkill) {
  SkillRepository repo;
  EXPECT_FALSE(
      repo.UninstallSkill("nonexistent_skill"));
}

TEST_F(SkillRepositoryTest, InvalidConfig) {
  std::ofstream f(config_path_);
  f << "not valid json" << std::endl;
  f.close();

  SkillRepository repo;
  EXPECT_TRUE(repo.Initialize(config_path_));
  EXPECT_FALSE(repo.IsEnabled());
}
