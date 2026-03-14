#include <gtest/gtest.h>

#include <cstdio>
#include <fstream>
#include <memory>

#include "channel.hh"
#include "channel_factory.hh"
#include "channel_registry.hh"

using namespace tizenclaw;

namespace {

constexpr char kTestConfigPath[] =
    "/tmp/test_channels.json";

void WriteTestConfig(const std::string& json) {
  std::ofstream f(kTestConfigPath);
  f << json;
}

void CleanupTestConfig() {
  std::remove(kTestConfigPath);
}

}  // namespace


// When channels.json is missing, factory should
// register fallback channels (mcp + web_dashboard)
TEST(ChannelFactoryTest, MissingConfigFallback) {
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      "/tmp/nonexistent_channels.json",
      nullptr, nullptr, reg);

  // Should have at least 2 fallback channels
  EXPECT_GE(reg.Size(), 2u);
  EXPECT_NE(reg.Get("mcp"), nullptr);
  EXPECT_NE(reg.Get("web_dashboard"), nullptr);
}

// Invalid JSON should still result in fallback
TEST(ChannelFactoryTest, InvalidJsonFallback) {
  WriteTestConfig("{ this is not valid json }}}");
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      kTestConfigPath, nullptr, nullptr, reg);
  EXPECT_GE(reg.Size(), 2u);
  CleanupTestConfig();
}

// Empty channels array = no channels registered
TEST(ChannelFactoryTest, EmptyChannelsArray) {
  WriteTestConfig("{\"channels\": []}");
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      kTestConfigPath, nullptr, nullptr, reg);
  EXPECT_EQ(reg.Size(), 0u);
  CleanupTestConfig();
}

// Disabled channels should not be registered
TEST(ChannelFactoryTest, DisabledChannelSkipped) {
  WriteTestConfig(R"({
    "channels": [
      {"name": "mcp", "enabled": false},
      {"name": "web_dashboard", "enabled": true}
    ]
  })");
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      kTestConfigPath, nullptr, nullptr, reg);
  EXPECT_EQ(reg.Get("mcp"), nullptr);
  EXPECT_NE(reg.Get("web_dashboard"), nullptr);
  CleanupTestConfig();
}

// Channel with config_file that doesn't exist
// should be skipped
TEST(ChannelFactoryTest,
     MissingExternalConfigSkipped) {
  WriteTestConfig(R"({
    "channels": [
      {"name": "mcp", "enabled": true},
      {
        "name": "telegram",
        "enabled": true,
        "config_file": "nonexistent_cfg.json",
        "required_keys": ["bot_token"]
      }
    ]
  })");
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      kTestConfigPath, nullptr, nullptr, reg);
  EXPECT_NE(reg.Get("mcp"), nullptr);
  EXPECT_EQ(reg.Get("telegram"), nullptr);
  CleanupTestConfig();
}

// Unknown channel name should be ignored
TEST(ChannelFactoryTest, UnknownChannelIgnored) {
  WriteTestConfig(R"({
    "channels": [
      {"name": "mcp", "enabled": true},
      {"name": "unknown_channel", "enabled": true}
    ]
  })");
  ChannelRegistry reg;
  ChannelFactory::CreateFromConfig(
      kTestConfigPath, nullptr, nullptr, reg);
  EXPECT_NE(reg.Get("mcp"), nullptr);
  EXPECT_EQ(reg.Get("unknown_channel"), nullptr);
  CleanupTestConfig();
}
