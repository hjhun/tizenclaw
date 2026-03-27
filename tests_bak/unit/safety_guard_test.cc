#include <gtest/gtest.h>
#include <fstream>
#include <unistd.h>
#include "safety_guard.hh"

using namespace tizenclaw;

class SafetyGuardTest : public ::testing::Test {
 protected:
  void SetUp() override {
    guard_ = new SafetyGuard();
    bounds_path_ = std::string("test_safety_bounds_") +
        ::testing::UnitTest::GetInstance()
            ->current_test_info()->name() + ".json";
    profile_path_ = std::string("test_device_profile_") +
        ::testing::UnitTest::GetInstance()
            ->current_test_info()->name() + ".json";
  }

  void TearDown() override {
    delete guard_;
    unlink(bounds_path_.c_str());
    unlink(profile_path_.c_str());
  }

  SafetyGuard* guard_;
  std::string bounds_path_;
  std::string profile_path_;
};

TEST_F(SafetyGuardTest, DefaultAllowsAll) {
  // Without config, all tools allowed
  nlohmann::json args = {{"temperature", 300}};
  auto result = guard_->Validate(
      "control_oven", args);
  EXPECT_TRUE(result.allowed);
}

TEST_F(SafetyGuardTest, LoadConfigSuccess) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "control_oven": [
        {
          "param": "temperature",
          "min": 0,
          "max": 250,
          "unit": "celsius",
          "description": "Max safe oven temp"
        }
      ]
    },
    "rate_limits": {},
    "confirmation_required": []
  })" << std::endl;
  f.close();

  EXPECT_TRUE(guard_->LoadConfig(bounds_path_));
}

TEST_F(SafetyGuardTest, BlocksExceedingMaxBound) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "control_oven": [
        {
          "param": "temperature",
          "min": 0,
          "max": 250,
          "unit": "celsius",
          "description": "Max safe oven temp"
        }
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  nlohmann::json args = {{"temperature", 300}};
  auto result = guard_->Validate(
      "control_oven", args);
  EXPECT_FALSE(result.allowed);
  EXPECT_NE(result.reason.find("exceeds"),
            std::string::npos);
}

TEST_F(SafetyGuardTest, BlocksBelowMinBound) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "set_temperature": [
        {
          "param": "temperature",
          "min": -25,
          "max": 10,
          "unit": "celsius"
        }
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  nlohmann::json args = {{"temperature", -30}};
  auto result = guard_->Validate(
      "set_temperature", args);
  EXPECT_FALSE(result.allowed);
  EXPECT_NE(result.reason.find("below"),
            std::string::npos);
}

TEST_F(SafetyGuardTest, AllowsWithinBounds) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "control_oven": [
        {
          "param": "temperature",
          "min": 0,
          "max": 250,
          "unit": "celsius"
        }
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  nlohmann::json args = {{"temperature", 180}};
  auto result = guard_->Validate(
      "control_oven", args);
  EXPECT_TRUE(result.allowed);
}

TEST_F(SafetyGuardTest, DeviceExcludedTools) {
  std::ofstream f(profile_path_);
  f << R"({
    "device_type": "refrigerator",
    "capabilities": ["temperature_control"],
    "excluded_tools": ["get_battery_info", "play_tone"]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadDeviceProfile(
      profile_path_));

  EXPECT_TRUE(guard_->IsExcludedTool(
      "get_battery_info"));
  EXPECT_TRUE(guard_->IsExcludedTool("play_tone"));
  EXPECT_FALSE(guard_->IsExcludedTool(
      "set_temperature"));
}

TEST_F(SafetyGuardTest,
       ExcludedToolBlockedByValidate) {
  std::ofstream f(profile_path_);
  f << R"({
    "device_type": "oven",
    "excluded_tools": ["get_battery_info"]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadDeviceProfile(
      profile_path_));

  auto result = guard_->Validate(
      "get_battery_info", {});
  EXPECT_FALSE(result.allowed);
  EXPECT_NE(result.reason.find("not available"),
            std::string::npos);
}

TEST_F(SafetyGuardTest, ActionRateLimit) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {},
    "rate_limits": {
      "preheat_oven": {
        "max_calls": 2,
        "window_seconds": 60
      }
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  // First two calls OK
  EXPECT_FALSE(guard_->CheckActionRateLimit(
      "preheat_oven"));
  EXPECT_FALSE(guard_->CheckActionRateLimit(
      "preheat_oven"));
  // Third call should be rate limited
  EXPECT_TRUE(guard_->CheckActionRateLimit(
      "preheat_oven"));
}

TEST_F(SafetyGuardTest,
       NoRateLimitForUnknownTool) {
  // No rate rule loaded = no limit
  EXPECT_FALSE(guard_->CheckActionRateLimit(
      "unknown_tool"));
}

TEST_F(SafetyGuardTest, ClampToSafeMax) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "control_oven": [
        {"param": "temperature", "min": 0, "max": 250}
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  auto [val, clamped] = guard_->ClampToSafe(
      "control_oven", "temperature", 300);
  EXPECT_TRUE(clamped);
  EXPECT_DOUBLE_EQ(val, 250.0);
}

TEST_F(SafetyGuardTest, ClampToSafeMin) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "set_temp": [
        {"param": "temperature", "min": -25, "max": 10}
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  auto [val, clamped] = guard_->ClampToSafe(
      "set_temp", "temperature", -30);
  EXPECT_TRUE(clamped);
  EXPECT_DOUBLE_EQ(val, -25.0);
}

TEST_F(SafetyGuardTest, ClampNotNeeded) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "control_oven": [
        {"param": "temperature", "min": 0, "max": 250}
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  auto [val, clamped] = guard_->ClampToSafe(
      "control_oven", "temperature", 180);
  EXPECT_FALSE(clamped);
  EXPECT_DOUBLE_EQ(val, 180.0);
}

TEST_F(SafetyGuardTest,
       ConfirmationRequired) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {},
    "confirmation_required": ["preheat_oven", "start_wash"]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  auto result1 = guard_->Validate(
      "preheat_oven", {});
  EXPECT_TRUE(result1.allowed);
  EXPECT_TRUE(result1.requires_confirmation);

  auto result2 = guard_->Validate(
      "get_battery_info", {});
  EXPECT_TRUE(result2.allowed);
  EXPECT_FALSE(result2.requires_confirmation);
}

TEST_F(SafetyGuardTest,
       MultipleBoundsPerTool) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "start_wash": [
        {"param": "spin_rpm", "min": 0, "max": 1400, "unit": "rpm"},
        {"param": "temperature", "min": 0, "max": 95, "unit": "celsius"}
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  // RPM OK, temp OK
  nlohmann::json ok_args = {
      {"spin_rpm", 1200}, {"temperature", 60}};
  auto r1 = guard_->Validate("start_wash", ok_args);
  EXPECT_TRUE(r1.allowed);

  // RPM too high
  nlohmann::json bad_rpm = {
      {"spin_rpm", 1800}, {"temperature", 60}};
  auto r2 = guard_->Validate("start_wash", bad_rpm);
  EXPECT_FALSE(r2.allowed);

  // Temp too high
  nlohmann::json bad_temp = {
      {"spin_rpm", 800}, {"temperature", 100}};
  auto r3 = guard_->Validate(
      "start_wash", bad_temp);
  EXPECT_FALSE(r3.allowed);
}

TEST_F(SafetyGuardTest, DeviceProfileMergesBounds) {
  // Load base bounds
  std::ofstream fb(bounds_path_);
  fb << R"({"bounds": {}})" << std::endl;
  fb.close();
  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  // Device profile adds its own bounds
  std::ofstream fp(profile_path_);
  fp << R"({
    "device_type": "oven",
    "safety_bounds": {
      "control_oven": [
        {"param": "temperature", "min": 0, "max": 230}
      ]
    }
  })" << std::endl;
  fp.close();

  ASSERT_TRUE(guard_->LoadDeviceProfile(
      profile_path_));

  nlohmann::json args = {{"temperature", 240}};
  auto result = guard_->Validate(
      "control_oven", args);
  EXPECT_FALSE(result.allowed);
}

TEST_F(SafetyGuardTest, GetStatusJson) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "tool_a": [{"param": "x", "min": 0, "max": 100}]
    },
    "rate_limits": {
      "tool_a": {"max_calls": 5, "window_seconds": 60}
    },
    "confirmation_required": ["tool_a"]
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  auto status = guard_->GetStatusJson();
  EXPECT_TRUE(status["config_loaded"].get<bool>());
  EXPECT_EQ(status["bounds_count"].get<int>(), 1);
  EXPECT_EQ(status["rate_rules_count"].get<int>(), 1);
  EXPECT_EQ(
      status["confirmation_tools_count"].get<int>(),
      1);
}

TEST_F(SafetyGuardTest,
       MissingConfigReturnsFalse) {
  EXPECT_FALSE(guard_->LoadConfig(
      "/nonexistent/path.json"));
}

TEST_F(SafetyGuardTest, StringParamHandled) {
  std::ofstream f(bounds_path_);
  f << R"({
    "bounds": {
      "set_vol": [
        {"param": "level", "min": 0, "max": 100}
      ]
    }
  })" << std::endl;
  f.close();

  ASSERT_TRUE(guard_->LoadConfig(bounds_path_));

  // String that can be parsed as number
  nlohmann::json args = {{"level", "150"}};
  auto result = guard_->Validate("set_vol", args);
  EXPECT_FALSE(result.allowed);
}
