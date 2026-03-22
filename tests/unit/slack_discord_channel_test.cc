#include <gtest/gtest.h>

#include "slack_channel.hh"
#include "discord_channel.hh"

using namespace tizenclaw;


TEST(SlackChannelTest, GetName) {
    SlackChannel ch(nullptr);
    EXPECT_EQ(ch.GetName(), "slack");
}

TEST(SlackChannelTest, StartWithoutConfig) {
    SlackChannel ch(nullptr);
    EXPECT_FALSE(ch.Start());
    EXPECT_FALSE(ch.IsRunning());
}

TEST(SlackChannelTest, StopWhenNotRunning) {
    SlackChannel ch(nullptr);
    ch.Stop();  // Should not crash
    EXPECT_FALSE(ch.IsRunning());
}

TEST(DiscordChannelTest, GetName) {
    DiscordChannel ch(nullptr);
    EXPECT_EQ(ch.GetName(), "discord");
}

TEST(DiscordChannelTest, StartWithoutConfig) {
    DiscordChannel ch(nullptr);
    EXPECT_FALSE(ch.Start());
    EXPECT_FALSE(ch.IsRunning());
}

TEST(DiscordChannelTest, StopWhenNotRunning) {
    DiscordChannel ch(nullptr);
    ch.Stop();  // Should not crash
    EXPECT_FALSE(ch.IsRunning());
}
