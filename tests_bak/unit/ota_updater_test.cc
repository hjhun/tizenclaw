#include <gtest/gtest.h>
#include <json.hpp>
#include <fstream>
#include <filesystem>
#include "ota_updater.hh"

namespace fs = std::filesystem;
using namespace tizenclaw;

class OtaUpdaterTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir_ = "/tmp/ota_test_" +
        std::to_string(getpid());
    skills_dir_ = test_dir_ + "/tools/skills";
    fs::create_directories(skills_dir_);

    updater_ = std::make_unique<OtaUpdater>(
        skills_dir_, [this]() {
          reload_called_ = true;
        });
  }

  void TearDown() override {
    std::error_code ec;
    fs::remove_all(test_dir_, ec);
  }

  void CreateSkill(
      const std::string& name,
      const std::string& version) {
    std::string dir =
        skills_dir_ + "/" + name;
    fs::create_directories(dir);
    nlohmann::json manifest = {
        {"name", name},
        {"version", version},
        {"description", "Test skill"}
    };
    std::ofstream f(
        dir + "/manifest.json");
    f << manifest.dump(2);
  }

  std::string test_dir_;
  std::string skills_dir_;
  std::unique_ptr<OtaUpdater> updater_;
  bool reload_called_ = false;
};

// --- Version Comparison ---

TEST_F(OtaUpdaterTest,
       VersionCompareNewer) {
  EXPECT_TRUE(
      OtaUpdater::IsNewerVersion(
          "1.0.0", "1.0.1"));
  EXPECT_TRUE(
      OtaUpdater::IsNewerVersion(
          "1.0.0", "1.1.0"));
  EXPECT_TRUE(
      OtaUpdater::IsNewerVersion(
          "1.0.0", "2.0.0"));
}

TEST_F(OtaUpdaterTest,
       VersionCompareEqual) {
  EXPECT_FALSE(
      OtaUpdater::IsNewerVersion(
          "1.0.0", "1.0.0"));
}

TEST_F(OtaUpdaterTest,
       VersionCompareOlder) {
  EXPECT_FALSE(
      OtaUpdater::IsNewerVersion(
          "2.0.0", "1.0.0"));
  EXPECT_FALSE(
      OtaUpdater::IsNewerVersion(
          "1.1.0", "1.0.0"));
}

TEST_F(OtaUpdaterTest,
       VersionComparePartial) {
  EXPECT_TRUE(
      OtaUpdater::IsNewerVersion(
          "1", "2"));
  EXPECT_TRUE(
      OtaUpdater::IsNewerVersion(
          "1.0", "1.1"));
}

// --- Manifest Parsing ---

TEST_F(OtaUpdaterTest,
       ParseManifestBasic) {
  CreateSkill("weather", "1.0.0");

  std::string manifest = R"({
    "skills": [
      {
        "name": "weather",
        "version": "1.1.0",
        "url": "https://example.com/weather.tar.gz",
        "sha256": "abc123"
      }
    ]
  })";

  auto updates =
      updater_->ParseManifest(
          manifest, skills_dir_);
  ASSERT_EQ(updates.size(), 1u);
  EXPECT_EQ(updates[0].name, "weather");
  EXPECT_EQ(updates[0].local_version,
            "1.0.0");
  EXPECT_EQ(updates[0].remote_version,
            "1.1.0");
  EXPECT_TRUE(
      updates[0].update_available);
}

TEST_F(OtaUpdaterTest,
       ParseManifestNoUpdate) {
  CreateSkill("timer", "2.0.0");

  std::string manifest = R"({
    "skills": [
      {
        "name": "timer",
        "version": "1.0.0"
      }
    ]
  })";

  auto updates =
      updater_->ParseManifest(
          manifest, skills_dir_);
  ASSERT_EQ(updates.size(), 1u);
  EXPECT_FALSE(
      updates[0].update_available);
}

TEST_F(OtaUpdaterTest,
       ParseManifestNewSkill) {
  // No local "newskill" directory
  std::string manifest = R"({
    "skills": [
      {
        "name": "newskill",
        "version": "1.0.0"
      }
    ]
  })";

  auto updates =
      updater_->ParseManifest(
          manifest, skills_dir_);
  ASSERT_EQ(updates.size(), 1u);
  // local_version=0.0.0, remote=1.0.0
  EXPECT_TRUE(
      updates[0].update_available);
}

TEST_F(OtaUpdaterTest,
       ParseManifestInvalid) {
  auto updates =
      updater_->ParseManifest(
          "not json", skills_dir_);
  EXPECT_TRUE(updates.empty());
}

TEST_F(OtaUpdaterTest,
       ParseManifestEmpty) {
  auto updates =
      updater_->ParseManifest(
          "{}", skills_dir_);
  EXPECT_TRUE(updates.empty());
}

// --- Config Loading ---

TEST_F(OtaUpdaterTest, LoadConfigValid) {
  std::string cfg_path =
      test_dir_ + "/ota_config.json";
  nlohmann::json cfg = {
      {"manifest_url",
       "https://example.com/manifest.json"},
      {"auto_check_interval_hours", 12},
      {"auto_update", false}
  };
  std::ofstream f(cfg_path);
  f << cfg.dump();
  f.close();

  EXPECT_TRUE(
      updater_->LoadConfig(cfg_path));
  EXPECT_EQ(updater_->GetManifestUrl(),
      "https://example.com/manifest.json");
}

TEST_F(OtaUpdaterTest,
       LoadConfigMissing) {
  EXPECT_FALSE(
      updater_->LoadConfig(
          "/nonexistent/path"));
}

// --- Rollback ---

TEST_F(OtaUpdaterTest,
       RollbackNoBackup) {
  std::string result =
      updater_->RollbackSkill("noexist");
  auto j = nlohmann::json::parse(result);
  EXPECT_TRUE(j.contains("error"));
}

// --- CheckForUpdates without URL ---

TEST_F(OtaUpdaterTest,
       CheckWithoutUrl) {
  std::string result =
      updater_->CheckForUpdates();
  auto j = nlohmann::json::parse(result);
  EXPECT_TRUE(j.contains("error"));
}

// --- UpdateSkill without URL ---

TEST_F(OtaUpdaterTest,
       UpdateWithoutUrl) {
  std::string result =
      updater_->UpdateSkill("test");
  auto j = nlohmann::json::parse(result);
  EXPECT_TRUE(j.contains("error"));
}
