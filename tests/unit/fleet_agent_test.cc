#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>
#include "fleet_agent.hh"

using namespace tizenclaw;

class FleetAgentTest : public ::testing::Test {
 protected:
  void SetUp() override {
    config_path_ =
        std::string("test_fleet_config_") +
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

TEST_F(FleetAgentTest, DisabledByDefault) {
  FleetAgent agent;
  EXPECT_TRUE(
      agent.Initialize("/nonexistent.json"));
  EXPECT_FALSE(agent.IsEnabled());
}

TEST_F(FleetAgentTest, EnabledViaConfig) {
  std::ofstream f(config_path_);
  f << R"({
    "enabled": true,
    "fleet_server_url": "https://test.example.com",
    "device_name": "Test Device",
    "device_group": "test_group"
  })" << std::endl;
  f.close();

  FleetAgent agent;
  EXPECT_TRUE(agent.Initialize(config_path_));
  EXPECT_TRUE(agent.IsEnabled());
}

TEST_F(FleetAgentTest, GetDeviceInfo) {
  std::ofstream f(config_path_);
  f << R"({
    "enabled": true,
    "fleet_server_url": "https://test.example.com",
    "device_name": "My TV",
    "device_group": "living_room"
  })" << std::endl;
  f.close();

  FleetAgent agent;
  agent.Initialize(config_path_);
  auto info = agent.GetDeviceInfo();
  EXPECT_EQ(info["device_name"], "My TV");
  EXPECT_EQ(info["device_group"], "living_room");
}

TEST_F(FleetAgentTest,
       StartStopWhenDisabled) {
  FleetAgent agent;
  agent.Initialize("/nonexistent.json");
  // Should be no-op when disabled
  agent.Start();
  agent.Stop();
}

TEST_F(FleetAgentTest, HeartbeatStatus) {
  FleetAgent agent;
  agent.Initialize("/nonexistent.json");
  auto status = agent.GetHeartbeatStatus();
  EXPECT_FALSE(status["running"].get<bool>());
  EXPECT_EQ(status["last_heartbeat_time"], 0);
}

TEST_F(FleetAgentTest, InvalidConfig) {
  std::ofstream f(config_path_);
  f << "not valid json" << std::endl;
  f.close();

  FleetAgent agent;
  EXPECT_TRUE(agent.Initialize(config_path_));
  EXPECT_FALSE(agent.IsEnabled());
}
