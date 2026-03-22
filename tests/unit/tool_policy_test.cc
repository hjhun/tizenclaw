#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include "tool_policy.hh"

using namespace tizenclaw;


class ToolPolicyTest : public ::testing::Test {
protected:
    void SetUp() override {
        policy = new ToolPolicy();
        config_path_ = std::string("test_tool_policy_") + ::testing::UnitTest::GetInstance()->current_test_info()->name() + ".json";
    }

    void TearDown() override {
        delete policy;
        unlink(config_path_.c_str());
    }

    ToolPolicy* policy;
    std::string config_path_;
};

TEST_F(ToolPolicyTest, DefaultPolicyAllowsAll) {
    // Without loading config, all tools allowed
    nlohmann::json args = {{"app_id", "test"}};
    std::string violation =
        policy->CheckPolicy(
            "session1", "launch_app", args);
    EXPECT_TRUE(violation.empty());
}

TEST_F(ToolPolicyTest, LoadConfigFromFile) {
    std::ofstream f(config_path_);
    f << R"({
      "max_repeat_count": 2,
      "blocked_skills": ["dangerous_tool"],
      "risk_overrides": {
        "launch_app": "high"
      }
    })" << std::endl;
    f.close();

    EXPECT_TRUE(policy->LoadConfig(
        config_path_));
}

TEST_F(ToolPolicyTest, BlockedSkillRejected) {
    std::ofstream f(config_path_);
    f << R"({
      "blocked_skills": ["blocked_tool"]
    })" << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    std::string violation =
        policy->CheckPolicy(
            "s1", "blocked_tool", {});
    EXPECT_FALSE(violation.empty());
    EXPECT_NE(violation.find("blocked"),
              std::string::npos);
}

TEST_F(ToolPolicyTest, LoopDetectionBlocks) {
    std::ofstream f(config_path_);
    f << R"({"max_repeat_count": 2})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    nlohmann::json args = {{"app_id", "test"}};

    // First two calls should pass
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());

    // Third call should be blocked (loop)
    std::string violation =
        policy->CheckPolicy(
            "s1", "launch_app", args);
    EXPECT_FALSE(violation.empty());
    EXPECT_NE(violation.find("loop"),
              std::string::npos);
}

TEST_F(ToolPolicyTest,
       DifferentArgsNotBlocked) {
    std::ofstream f(config_path_);
    f << R"({"max_repeat_count": 1})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    nlohmann::json args1 = {{"app_id", "app1"}};
    nlohmann::json args2 = {{"app_id", "app2"}};

    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args1).empty());
    // Different args = different hash
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args2).empty());
}

TEST_F(ToolPolicyTest,
       DifferentSessionsIndependent) {
    std::ofstream f(config_path_);
    f << R"({"max_repeat_count": 1})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    nlohmann::json args = {{"app_id", "test"}};

    // Session 1: first call
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());

    // Session 2: independent tracking
    EXPECT_TRUE(policy->CheckPolicy(
        "s2", "launch_app", args).empty());
}

TEST_F(ToolPolicyTest, ResetSessionClears) {
    std::ofstream f(config_path_);
    f << R"({"max_repeat_count": 1})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    nlohmann::json args = {{"app_id", "test"}};

    // Use up the limit
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());
    EXPECT_FALSE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());

    // Reset session tracking
    policy->ResetSession("s1");

    // Should be allowed again
    EXPECT_TRUE(policy->CheckPolicy(
        "s1", "launch_app", args).empty());
}

TEST_F(ToolPolicyTest,
       ManifestRiskLevelLoaded) {
    nlohmann::json manifest = {
        {"name", "launch_app"},
        {"risk_level", "high"}
    };

    policy->LoadManifestRiskLevel(
        "launch_app", manifest);

    EXPECT_EQ(policy->GetRiskLevel("launch_app"),
              RiskLevel::kHigh);
}

TEST_F(ToolPolicyTest,
       DefaultRiskLevelIsNormal) {
    EXPECT_EQ(
        policy->GetRiskLevel("unknown_tool"),
        RiskLevel::kNormal);
}

TEST_F(ToolPolicyTest,
       RiskLevelToStringWorks) {
    EXPECT_EQ(ToolPolicy::RiskLevelToString(
        RiskLevel::kLow), "low");
    EXPECT_EQ(ToolPolicy::RiskLevelToString(
        RiskLevel::kNormal), "normal");
    EXPECT_EQ(ToolPolicy::RiskLevelToString(
        RiskLevel::kHigh), "high");
}

TEST_F(ToolPolicyTest,
       MissingConfigUsesDefaults) {
    EXPECT_TRUE(policy->LoadConfig(
        "/nonexistent/path.json"));
}

TEST_F(ToolPolicyTest,
       DefaultMaxIterations) {
    // Default should be 5
    EXPECT_EQ(policy->GetMaxIterations(), 5);
}

TEST_F(ToolPolicyTest,
       ConfigMaxIterations) {
    std::ofstream f(config_path_);
    f << R"({"max_iterations": 10})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    EXPECT_EQ(policy->GetMaxIterations(), 10);
}

TEST_F(ToolPolicyTest,
       IdleDetectionTriggersAfterWindow) {
    // Same output 3 times -> idle
    std::string same_output =
        "tool1:result;tool2:result;";

    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", same_output));
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", same_output));
    // 3rd time -> idle detected
    EXPECT_TRUE(policy->CheckIdleProgress(
        "s1", same_output));
}

TEST_F(ToolPolicyTest,
       DifferentOutputsNotIdle) {
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", "output_a"));
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", "output_b"));
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", "output_c"));
}

TEST_F(ToolPolicyTest,
       ResetIdleTrackingClears) {
    std::string same = "same_output";

    (void)policy->CheckIdleProgress("s1", same);
    (void)policy->CheckIdleProgress("s1", same);

    // Reset before hitting window
    policy->ResetIdleTracking("s1");

    // After reset, need 3 more
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", same));
    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", same));
    EXPECT_TRUE(policy->CheckIdleProgress(
        "s1", same));
}

TEST_F(ToolPolicyTest,
       ResetSessionClearsIdle) {
    std::string same = "same_output";

    (void)policy->CheckIdleProgress("s1", same);
    (void)policy->CheckIdleProgress("s1", same);

    // ResetSession should also clear idle
    policy->ResetSession("s1");

    EXPECT_FALSE(policy->CheckIdleProgress(
        "s1", same));
}

TEST_F(ToolPolicyTest,
       LoadConfigWithAliases) {
    std::ofstream f(config_path_);
    f << R"({
      "aliases": {
        "control_display": "action_brightness",
        "control_volume": "action_volume"
      }
    })" << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    auto& aliases = policy->GetAliases();
    EXPECT_EQ(aliases.size(), 2u);
    EXPECT_EQ(aliases.at("control_display"),
              "action_brightness");
    EXPECT_EQ(aliases.at("control_volume"),
              "action_volume");
}

TEST_F(ToolPolicyTest,
       MissingAliasesFieldUsesEmpty) {
    std::ofstream f(config_path_);
    f << R"({"max_repeat_count": 3})"
      << std::endl;
    f.close();

    ASSERT_TRUE(policy->LoadConfig(config_path_));

    auto& aliases = policy->GetAliases();
    EXPECT_TRUE(aliases.empty());
}
