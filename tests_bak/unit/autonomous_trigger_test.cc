#include <gtest/gtest.h>
#include <chrono>
#include <fstream>
#include <thread>
#include <atomic>
#include <sys/stat.h>
#include "autonomous_trigger.hh"
#include "event_bus.hh"

using namespace tizenclaw;

class AutonomousTriggerTest : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir = "/tmp/tizenclaw_trigger_test";
    mkdir(test_dir.c_str(), 0755);

    EventBus::GetInstance().Start();
    trigger = std::make_unique<AutonomousTrigger>(
        nullptr, nullptr);
  }

  void TearDown() override {
    trigger.reset();
    EventBus::GetInstance().Stop();
    std::string cmd = "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  std::string WriteConfig(const std::string& json) {
    std::string path = test_dir + "/trigger.json";
    std::ofstream f(path);
    f << json;
    f.close();
    return path;
  }

  std::string test_dir;
  std::unique_ptr<AutonomousTrigger> trigger;
};

// -------------------------------------------
// LoadRules Tests
// -------------------------------------------
TEST_F(AutonomousTriggerTest,
       LoadRulesSuccess) {
  auto path = WriteConfig(R"({
    "enabled": true,
    "max_evaluations_per_hour": 5,
    "trigger_rules": [
      {
        "name": "test_rule",
        "event_type": "battery.level_changed",
        "condition": {"level": {"$lt": 20}},
        "cooldown_minutes": 10,
        "action": "evaluate"
      }
    ]
  })");

  EXPECT_TRUE(trigger->LoadRules(path));
  EXPECT_TRUE(trigger->IsEnabled());

  auto rules = trigger->ListRules();
  EXPECT_EQ(rules.size(), 1u);
  EXPECT_EQ(rules[0]["name"], "test_rule");
}

TEST_F(AutonomousTriggerTest,
       LoadRulesDisabled) {
  auto path = WriteConfig(R"({
    "enabled": false,
    "trigger_rules": []
  })");

  EXPECT_TRUE(trigger->LoadRules(path));
  EXPECT_FALSE(trigger->IsEnabled());
}

TEST_F(AutonomousTriggerTest,
       LoadRulesFileNotFound) {
  EXPECT_FALSE(trigger->LoadRules(
      "/nonexistent/path.json"));
}

TEST_F(AutonomousTriggerTest,
       LoadRulesMultiple) {
  auto path = WriteConfig(R"({
    "enabled": true,
    "trigger_rules": [
      {"name": "r1", "event_type": "e1",
       "action": "evaluate"},
      {"name": "r2", "event_type": "e2",
       "action": "direct",
       "direct_prompt": "handle e2"},
      {"name": "r3", "event_type": "e3",
       "action": "evaluate",
       "cooldown_minutes": 60}
    ]
  })");

  EXPECT_TRUE(trigger->LoadRules(path));
  auto rules = trigger->ListRules();
  EXPECT_EQ(rules.size(), 3u);
}

TEST_F(AutonomousTriggerTest,
       LoadRulesSkipsInvalid) {
  auto path = WriteConfig(R"({
    "enabled": true,
    "trigger_rules": [
      {"name": "valid", "event_type": "e1",
       "action": "evaluate"},
      {"name": "", "event_type": "e2",
       "action": "evaluate"},
      {"name": "valid2", "event_type": "",
       "action": "evaluate"}
    ]
  })");

  EXPECT_TRUE(trigger->LoadRules(path));
  auto rules = trigger->ListRules();
  EXPECT_EQ(rules.size(), 1u);
}

// -------------------------------------------
// Condition Evaluation Tests
// -------------------------------------------
// We test MatchRule indirectly through Start/OnEvent

TEST_F(AutonomousTriggerTest,
       ListRulesReturnsCorrectFormat) {
  auto path = WriteConfig(R"({
    "enabled": true,
    "trigger_rules": [
      {
        "name": "battery_low",
        "event_type": "battery.level_changed",
        "condition": {"level": {"$lt": 15}},
        "cooldown_minutes": 30,
        "action": "evaluate"
      }
    ]
  })");

  trigger->LoadRules(path);
  auto rules = trigger->ListRules();

  ASSERT_EQ(rules.size(), 1u);
  EXPECT_EQ(rules[0]["name"], "battery_low");
  EXPECT_EQ(rules[0]["event_type"],
            "battery.level_changed");
  EXPECT_EQ(rules[0]["cooldown_minutes"], 30);
  EXPECT_EQ(rules[0]["action"], "evaluate");
  EXPECT_TRUE(rules[0].contains("condition"));
}

// -------------------------------------------
// Start/Stop Tests
// -------------------------------------------
TEST_F(AutonomousTriggerTest,
       StartWhenDisabledDoesNothing) {
  auto path = WriteConfig(R"({
    "enabled": false,
    "trigger_rules": []
  })");

  trigger->LoadRules(path);
  trigger->Start();

  // Should not crash, just no-op
  trigger->Stop();
}

TEST_F(AutonomousTriggerTest,
       StartStopCycle) {
  auto path = WriteConfig(R"({
    "enabled": true,
    "trigger_rules": [
      {"name": "test", "event_type": "test.event",
       "action": "evaluate"}
    ]
  })");

  trigger->LoadRules(path);
  trigger->Start();

  // Should be subscribable
  std::this_thread::sleep_for(
      std::chrono::milliseconds(100));

  trigger->Stop();
  // Double stop should be safe
  trigger->Stop();
}

// -------------------------------------------
// Config Defaults Test
// -------------------------------------------
TEST_F(AutonomousTriggerTest,
       DefaultConfigValues) {
  auto path = WriteConfig(R"({
    "trigger_rules": [
      {"name": "minimal", "event_type": "e1",
       "action": "evaluate"}
    ]
  })");

  trigger->LoadRules(path);
  EXPECT_FALSE(trigger->IsEnabled());

  auto rules = trigger->ListRules();
  EXPECT_EQ(rules.size(), 1u);
  // Default cooldown should be 10
  EXPECT_EQ(rules[0]["cooldown_minutes"], 10);
}
