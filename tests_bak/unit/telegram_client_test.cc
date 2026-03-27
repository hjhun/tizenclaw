#include <gtest/gtest.h>
#include "telegram_client.hh"
#include "agent_core.hh"

using namespace tizenclaw;


class TelegramClientTest : public ::testing::Test {
protected:
    void SetUp() override {
        agent_ = new AgentCore();
        client_ = new TelegramClient(agent_);
    }

    void TearDown() override {
        delete client_;
        delete agent_;
    }

    AgentCore* agent_;
    TelegramClient* client_;
};

// Start without config should fail gracefully
TEST_F(TelegramClientTest, StartWithoutConfigFails) {
    // Requires a config file. Since one doesn't exist
    // or has default tokens, Start() should return false.
    EXPECT_FALSE(client_->Start());
}

// Stop without start should be safe
TEST_F(TelegramClientTest, StopWithoutStart) {
    EXPECT_NO_THROW(client_->Stop());
}
