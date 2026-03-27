#include <gtest/gtest.h>
#include "capability_registry.hh"

using namespace tizenclaw;

class CapabilityRegistryTest : public ::testing::Test {
 protected:
  void SetUp() override {
    CapabilityRegistry::GetInstance().Clear();
  }

  void TearDown() override {
    CapabilityRegistry::GetInstance().Clear();
  }
};

TEST_F(CapabilityRegistryTest, RegisterAndGet) {
  Capability cap;
  cap.name = "test_tool";
  cap.description = "A test tool";
  cap.category = "testing";
  cap.source = CapabilitySource::kBuiltin;
  cap.contract.side_effect = SideEffect::kNone;

  CapabilityRegistry::GetInstance().Register(
      "test_tool", cap);

  auto* result =
      CapabilityRegistry::GetInstance().Get("test_tool");
  ASSERT_NE(result, nullptr);
  EXPECT_EQ(result->name, "test_tool");
  EXPECT_EQ(result->category, "testing");
  EXPECT_EQ(result->contract.side_effect,
            SideEffect::kNone);
}

TEST_F(CapabilityRegistryTest, GetNonExistent) {
  auto* result =
      CapabilityRegistry::GetInstance().Get("missing");
  EXPECT_EQ(result, nullptr);
}

TEST_F(CapabilityRegistryTest, Unregister) {
  Capability cap;
  cap.name = "temp_tool";
  CapabilityRegistry::GetInstance().Register(
      "temp_tool", cap);
  EXPECT_EQ(CapabilityRegistry::GetInstance().Size(),
            1u);

  CapabilityRegistry::GetInstance().Unregister(
      "temp_tool");
  EXPECT_EQ(CapabilityRegistry::GetInstance().Size(),
            0u);
  EXPECT_EQ(
      CapabilityRegistry::GetInstance().Get("temp_tool"),
      nullptr);
}

TEST_F(CapabilityRegistryTest, QueryByCategory) {
  Capability cap1;
  cap1.name = "tool_a";
  cap1.category = "network";
  CapabilityRegistry::GetInstance().Register(
      "tool_a", cap1);

  Capability cap2;
  cap2.name = "tool_b";
  cap2.category = "network";
  CapabilityRegistry::GetInstance().Register(
      "tool_b", cap2);

  Capability cap3;
  cap3.name = "tool_c";
  cap3.category = "device";
  CapabilityRegistry::GetInstance().Register(
      "tool_c", cap3);

  auto network =
      CapabilityRegistry::GetInstance().QueryByCategory(
          "network");
  EXPECT_EQ(network.size(), 2u);

  auto device =
      CapabilityRegistry::GetInstance().QueryByCategory(
          "device");
  EXPECT_EQ(device.size(), 1u);

  auto empty =
      CapabilityRegistry::GetInstance().QueryByCategory(
          "nonexistent");
  EXPECT_TRUE(empty.empty());
}

TEST_F(CapabilityRegistryTest, QueryBySideEffect) {
  Capability cap1;
  cap1.name = "getter";
  cap1.contract.side_effect = SideEffect::kNone;
  CapabilityRegistry::GetInstance().Register(
      "getter", cap1);

  Capability cap2;
  cap2.name = "setter";
  cap2.contract.side_effect = SideEffect::kReversible;
  CapabilityRegistry::GetInstance().Register(
      "setter", cap2);

  Capability cap3;
  cap3.name = "deleter";
  cap3.contract.side_effect =
      SideEffect::kIrreversible;
  CapabilityRegistry::GetInstance().Register(
      "deleter", cap3);

  auto read_only =
      CapabilityRegistry::GetInstance()
          .QueryBySideEffect(SideEffect::kNone);
  EXPECT_EQ(read_only.size(), 1u);
  EXPECT_EQ(read_only[0].name, "getter");

  auto reversible =
      CapabilityRegistry::GetInstance()
          .QueryBySideEffect(SideEffect::kReversible);
  EXPECT_EQ(reversible.size(), 1u);
  EXPECT_EQ(reversible[0].name, "setter");
}

TEST_F(CapabilityRegistryTest, QueryByPermission) {
  Capability cap1;
  cap1.name = "wifi_scan";
  cap1.contract.required_permissions = {"network"};
  CapabilityRegistry::GetInstance().Register(
      "wifi_scan", cap1);

  Capability cap2;
  cap2.name = "bt_scan";
  cap2.contract.required_permissions = {
      "bluetooth", "network"};
  CapabilityRegistry::GetInstance().Register(
      "bt_scan", cap2);

  auto network =
      CapabilityRegistry::GetInstance()
          .QueryByPermission("network");
  EXPECT_EQ(network.size(), 2u);

  auto bt =
      CapabilityRegistry::GetInstance()
          .QueryByPermission("bluetooth");
  EXPECT_EQ(bt.size(), 1u);
  EXPECT_EQ(bt[0].name, "bt_scan");
}

TEST_F(CapabilityRegistryTest, GetAllNames) {
  Capability cap1;
  cap1.name = "a";
  Capability cap2;
  cap2.name = "b";
  CapabilityRegistry::GetInstance().Register("a", cap1);
  CapabilityRegistry::GetInstance().Register("b", cap2);

  auto names =
      CapabilityRegistry::GetInstance().GetAllNames();
  EXPECT_EQ(names.size(), 2u);
}

TEST_F(CapabilityRegistryTest, GetCapabilitySummary) {
  Capability cap1;
  cap1.name = "getter";
  cap1.category = "device";
  cap1.contract.side_effect = SideEffect::kNone;
  cap1.contract.estimated_duration_ms = 2000;
  CapabilityRegistry::GetInstance().Register(
      "getter", cap1);

  Capability cap2;
  cap2.name = "sender";
  cap2.category = "notification";
  cap2.contract.side_effect =
      SideEffect::kIrreversible;
  cap2.contract.estimated_duration_ms = 3000;
  cap2.contract.required_permissions = {"network"};
  CapabilityRegistry::GetInstance().Register(
      "sender", cap2);

  auto summary =
      CapabilityRegistry::GetInstance()
          .GetCapabilitySummary();

  EXPECT_TRUE(summary.contains("categories"));
  EXPECT_TRUE(summary.contains("total_capabilities"));
  EXPECT_EQ(summary["total_capabilities"], 2);
  EXPECT_TRUE(
      summary.contains("side_effect_summary"));
  EXPECT_EQ(
      summary["side_effect_summary"]["none"], 1);
  EXPECT_EQ(
      summary["side_effect_summary"]["irreversible"],
      1);
}

TEST_F(CapabilityRegistryTest, ParseContract) {
  nlohmann::json j = {
      {"side_effect", "reversible"},
      {"max_retries", 3},
      {"retry_delay_ms", 2000},
      {"idempotent", true},
      {"estimated_duration_ms", 10000},
      {"execution_env", "container"},
      {"required_permissions",
       nlohmann::json::array(
           {"network", "bluetooth"})}};

  auto contract =
      CapabilityRegistry::ParseContract(j);
  EXPECT_EQ(contract.side_effect,
            SideEffect::kReversible);
  EXPECT_EQ(contract.max_retries, 3);
  EXPECT_EQ(contract.retry_delay_ms, 2000);
  EXPECT_TRUE(contract.idempotent);
  EXPECT_EQ(contract.estimated_duration_ms, 10000);
  EXPECT_EQ(contract.execution_env, "container");
  EXPECT_EQ(contract.required_permissions.size(), 2u);
}

TEST_F(CapabilityRegistryTest,
       ParseContractEmpty) {
  nlohmann::json j = nlohmann::json::object();
  auto contract =
      CapabilityRegistry::ParseContract(j);
  EXPECT_EQ(contract.side_effect,
            SideEffect::kUnknown);
  EXPECT_EQ(contract.max_retries, 0);
  EXPECT_FALSE(contract.idempotent);
}

TEST_F(CapabilityRegistryTest,
       ParseContractNonObject) {
  auto contract =
      CapabilityRegistry::ParseContract("invalid");
  EXPECT_EQ(contract.side_effect,
            SideEffect::kUnknown);
}

TEST_F(CapabilityRegistryTest,
       SideEffectStringConversion) {
  EXPECT_EQ(
      CapabilityRegistry::SideEffectToString(
          SideEffect::kNone),
      "read-only");
  EXPECT_EQ(
      CapabilityRegistry::SideEffectToString(
          SideEffect::kReversible),
      "reversible");
  EXPECT_EQ(
      CapabilityRegistry::SideEffectToString(
          SideEffect::kIrreversible),
      "irreversible");
  EXPECT_EQ(
      CapabilityRegistry::SideEffectToString(
          SideEffect::kUnknown),
      "unknown");
}

TEST_F(CapabilityRegistryTest,
       ParseSideEffectStrings) {
  EXPECT_EQ(
      CapabilityRegistry::ParseSideEffect("none"),
      SideEffect::kNone);
  EXPECT_EQ(
      CapabilityRegistry::ParseSideEffect("read-only"),
      SideEffect::kNone);
  EXPECT_EQ(
      CapabilityRegistry::ParseSideEffect(
          "reversible"),
      SideEffect::kReversible);
  EXPECT_EQ(
      CapabilityRegistry::ParseSideEffect(
          "irreversible"),
      SideEffect::kIrreversible);
  EXPECT_EQ(
      CapabilityRegistry::ParseSideEffect("garbage"),
      SideEffect::kUnknown);
}

TEST_F(CapabilityRegistryTest, ClearAll) {
  Capability cap;
  cap.name = "a";
  CapabilityRegistry::GetInstance().Register("a", cap);
  cap.name = "b";
  CapabilityRegistry::GetInstance().Register("b", cap);
  EXPECT_EQ(CapabilityRegistry::GetInstance().Size(),
            2u);

  CapabilityRegistry::GetInstance().Clear();
  EXPECT_EQ(CapabilityRegistry::GetInstance().Size(),
            0u);
}
