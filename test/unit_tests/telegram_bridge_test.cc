#include <gtest/gtest.h>
#include "telegram_bridge.hh"

class TelegramBridgeTest : public ::testing::Test {
protected:
    void SetUp() override {
        bridge = new TelegramBridge();
    }

    void TearDown() override {
        bridge->Stop();
        delete bridge;
    }

    TelegramBridge* bridge;
};

TEST_F(TelegramBridgeTest, StartWithoutConfig) {
    // Should fail gracefully when telegram_config.json is missing
    EXPECT_FALSE(bridge->Start());
}

TEST_F(TelegramBridgeTest, StopWithoutStart) {
    // Should not crash when Stop() is called without Start()
    bridge->Stop();
    EXPECT_FALSE(bridge->IsRunning());
}

TEST_F(TelegramBridgeTest, IsRunningWithoutStart) {
    // Should return false when not started
    EXPECT_FALSE(bridge->IsRunning());
}
