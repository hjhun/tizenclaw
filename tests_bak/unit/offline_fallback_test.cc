#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>
#include "offline_fallback.hh"

using namespace tizenclaw;

class OfflineFallbackTest : public ::testing::Test {
 protected:
  void SetUp() override {
    fb_ = new OfflineFallback();
    config_path_ = std::string("test_offline_fb_") +
        ::testing::UnitTest::GetInstance()
            ->current_test_info()->name() + ".json";
  }

  void TearDown() override {
    delete fb_;
    unlink(config_path_.c_str());
  }

  OfflineFallback* fb_;
  std::string config_path_;
};

TEST_F(OfflineFallbackTest, NoConfigNoMatch) {
  auto result = fb_->Match("check battery");
  EXPECT_FALSE(result.matched);
}

TEST_F(OfflineFallbackTest, LoadConfigSuccess) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "battery",
        "keywords": ["battery", "배터리"],
        "tool": "get_battery_info",
        "args": {},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  EXPECT_TRUE(fb_->LoadConfig(config_path_));
  EXPECT_EQ(fb_->GetRuleCount(), 1u);
}

TEST_F(OfflineFallbackTest,
       MatchesToolByKeyword) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "battery",
        "keywords": ["battery"],
        "tool": "get_battery_info",
        "args": {},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("Check my battery level");
  EXPECT_TRUE(result.matched);
  EXPECT_EQ(result.tool_name, "get_battery_info");
}

TEST_F(OfflineFallbackTest,
       CaseInsensitiveMatch) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "volume",
        "keywords": ["volume up"],
        "tool": "control_volume",
        "args": {"action": "up"},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("VOLUME UP please");
  EXPECT_TRUE(result.matched);
  EXPECT_EQ(result.tool_name, "control_volume");
}

TEST_F(OfflineFallbackTest,
       DirectResponseWorks) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "help",
        "keywords": ["help"],
        "tool": "",
        "response": "I'm in offline mode.",
        "priority": 1
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("I need help");
  EXPECT_TRUE(result.matched);
  EXPECT_TRUE(result.tool_name.empty());
  EXPECT_EQ(result.direct_response,
            "I'm in offline mode.");
}

TEST_F(OfflineFallbackTest,
       PriorityOrderRespected) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "low_priority",
        "keywords": ["volume"],
        "tool": "low_tool",
        "args": {},
        "priority": 1
      },
      {
        "name": "high_priority",
        "keywords": ["volume"],
        "tool": "high_tool",
        "args": {},
        "priority": 100
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("adjust volume");
  EXPECT_TRUE(result.matched);
  // Higher priority should win
  EXPECT_EQ(result.tool_name, "high_tool");
}

TEST_F(OfflineFallbackTest, NoMatchReturnsEmpty) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "battery",
        "keywords": ["battery"],
        "tool": "get_battery_info",
        "args": {},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("What is the weather?");
  EXPECT_FALSE(result.matched);
}

TEST_F(OfflineFallbackTest, KoreanKeywordMatch) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "battery_kr",
        "keywords": ["배터리"],
        "tool": "get_battery_info",
        "args": {},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("배터리 상태 확인");
  EXPECT_TRUE(result.matched);
  EXPECT_EQ(result.tool_name, "get_battery_info");
}

TEST_F(OfflineFallbackTest, DefaultArgsPassedThrough) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "vol_up",
        "keywords": ["volume up"],
        "tool": "control_volume",
        "args": {"action": "up", "value": 10},
        "priority": 10
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto result = fb_->Match("volume up");
  EXPECT_TRUE(result.matched);
  EXPECT_EQ(result.args["action"], "up");
  EXPECT_EQ(result.args["value"], 10);
}

TEST_F(OfflineFallbackTest, MissingConfigReturnsFalse) {
  EXPECT_FALSE(fb_->LoadConfig(
      "/nonexistent/path.json"));
}

TEST_F(OfflineFallbackTest, GetStatusJson) {
  std::ofstream f(config_path_);
  f << R"({
    "rules": [
      {
        "name": "test_rule",
        "keywords": ["test"],
        "tool": "test_tool",
        "args": {},
        "priority": 1
      }
    ]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(fb_->LoadConfig(config_path_));

  auto status = fb_->GetStatusJson();
  EXPECT_TRUE(status["config_loaded"].get<bool>());
  EXPECT_EQ(status["rule_count"].get<int>(), 1);
}
