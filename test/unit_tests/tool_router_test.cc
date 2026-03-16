#include <gtest/gtest.h>
#include "tool_router.hh"

using namespace tizenclaw;

class ToolRouterTest : public ::testing::Test {
 protected:
  ToolRouter router;
};

TEST_F(ToolRouterTest,
       ResolveReturnsOriginalWhenNoAlias) {
  auto result = router.Resolve("get_battery_info");
  EXPECT_EQ(result, "get_battery_info");
}

TEST_F(ToolRouterTest,
       ResolveRedirectsViaAlias) {
  nlohmann::json aliases = {
      {"control_display", "action_brightness"}};
  router.LoadAliases(aliases);

  auto result = router.Resolve("control_display");
  EXPECT_EQ(result, "action_brightness");
}

TEST_F(ToolRouterTest,
       ResolveRedirectsViaOverlap) {
  router.RegisterOverlap(
      "control_volume", "action_volume");

  auto result = router.Resolve("control_volume");
  EXPECT_EQ(result, "action_volume");
}

TEST_F(ToolRouterTest,
       AliasOverridesOverlap) {
  // Register overlap first
  router.RegisterOverlap(
      "my_tool", "auto_target");

  // Then load alias for the same tool
  nlohmann::json aliases = {
      {"my_tool", "manual_target"}};
  router.LoadAliases(aliases);

  // Alias should win
  auto result = router.Resolve("my_tool");
  EXPECT_EQ(result, "manual_target");
}

TEST_F(ToolRouterTest,
       OverlapSkippedWhenAliasExists) {
  // Load alias first
  nlohmann::json aliases = {
      {"my_tool", "alias_target"}};
  router.LoadAliases(aliases);

  // Try to register overlap — should be skipped
  router.RegisterOverlap(
      "my_tool", "overlap_target");

  auto result = router.Resolve("my_tool");
  EXPECT_EQ(result, "alias_target");
}

TEST_F(ToolRouterTest,
       SelfAliasIgnored) {
  nlohmann::json aliases = {
      {"same_tool", "same_tool"}};
  router.LoadAliases(aliases);

  EXPECT_FALSE(router.HasRedirect("same_tool"));
  EXPECT_EQ(router.Resolve("same_tool"),
            "same_tool");
}

TEST_F(ToolRouterTest,
       EmptyAliasValueIgnored) {
  nlohmann::json aliases = {
      {"tool_a", ""}};
  router.LoadAliases(aliases);

  EXPECT_FALSE(router.HasRedirect("tool_a"));
}

TEST_F(ToolRouterTest,
       HasRedirectWorks) {
  EXPECT_FALSE(
      router.HasRedirect("control_display"));

  nlohmann::json aliases = {
      {"control_display", "action_brightness"}};
  router.LoadAliases(aliases);

  EXPECT_TRUE(
      router.HasRedirect("control_display"));
  EXPECT_FALSE(
      router.HasRedirect("action_brightness"));
}

TEST_F(ToolRouterTest,
       GetAllRedirectsMerged) {
  router.RegisterOverlap("a", "b");

  nlohmann::json aliases = {{"c", "d"}};
  router.LoadAliases(aliases);

  auto all = router.GetAllRedirects();
  EXPECT_EQ(all.size(), 2u);
  EXPECT_EQ(all["a"], "b");
  EXPECT_EQ(all["c"], "d");
}

TEST_F(ToolRouterTest, ClearRemovesAll) {
  nlohmann::json aliases = {
      {"x", "y"}};
  router.LoadAliases(aliases);
  router.RegisterOverlap("a", "b");

  router.Clear();

  EXPECT_FALSE(router.HasRedirect("x"));
  EXPECT_FALSE(router.HasRedirect("a"));
  EXPECT_EQ(router.GetAllRedirects().size(), 0u);
}

TEST_F(ToolRouterTest,
       SourcePriorityOrder) {
  // Action should be highest priority (lowest num)
  EXPECT_LT(
      ToolRouter::SourcePriority(
          CapabilitySource::kAction),
      ToolRouter::SourcePriority(
          CapabilitySource::kBuiltin));
  EXPECT_LT(
      ToolRouter::SourcePriority(
          CapabilitySource::kBuiltin),
      ToolRouter::SourcePriority(
          CapabilitySource::kSystemCli));
  EXPECT_LT(
      ToolRouter::SourcePriority(
          CapabilitySource::kSystemCli),
      ToolRouter::SourcePriority(
          CapabilitySource::kSkill));
  EXPECT_LT(
      ToolRouter::SourcePriority(
          CapabilitySource::kSkill),
      ToolRouter::SourcePriority(
          CapabilitySource::kCli));
  EXPECT_LT(
      ToolRouter::SourcePriority(
          CapabilitySource::kCli),
      ToolRouter::SourcePriority(
          CapabilitySource::kRpk));
}

TEST_F(ToolRouterTest,
       NonStringAliasValueIgnored) {
  nlohmann::json aliases = {
      {"tool_a", 123},
      {"tool_b", "valid_target"}};
  router.LoadAliases(aliases);

  EXPECT_FALSE(router.HasRedirect("tool_a"));
  EXPECT_TRUE(router.HasRedirect("tool_b"));
}

TEST_F(ToolRouterTest,
       InvalidAliasJsonIgnored) {
  // Non-object JSON should not crash
  nlohmann::json aliases = "not_an_object";
  router.LoadAliases(aliases);

  EXPECT_EQ(router.GetAllRedirects().size(), 0u);
}

TEST_F(ToolRouterTest,
       SelfOverlapIgnored) {
  router.RegisterOverlap("same", "same");
  EXPECT_FALSE(router.HasRedirect("same"));
}
