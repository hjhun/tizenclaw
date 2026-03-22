#include <gtest/gtest.h>
#include <memory>

#include "channel.hh"
#include "channel_registry.hh"

using namespace tizenclaw;


// Mock channel for testing
class MockChannel : public Channel {
public:
    MockChannel(const std::string& name,
                bool start_ok = true)
        : name_(name),
          start_ok_(start_ok),
          running_(false) {}

    std::string GetName() const override {
      return name_;
    }
    bool Start() override {
      if (!start_ok_) return false;
      running_ = true;
      return true;
    }
    void Stop() override {
      running_ = false;
    }
    bool IsRunning() const override {
      return running_;
    }

    int start_count() const {
      return start_count_;
    }

private:
    std::string name_;
    bool start_ok_;
    bool running_;
    int start_count_ = 0;
};


TEST(ChannelRegistryTest, RegisterAndList) {
    ChannelRegistry reg;
    EXPECT_EQ(reg.Size(), 0u);

    reg.Register(
        std::make_unique<MockChannel>("alpha"));
    reg.Register(
        std::make_unique<MockChannel>("beta"));

    EXPECT_EQ(reg.Size(), 2u);

    auto names = reg.ListChannels();
    ASSERT_EQ(names.size(), 2u);
    EXPECT_EQ(names[0], "alpha");
    EXPECT_EQ(names[1], "beta");
}

TEST(ChannelRegistryTest, GetByName) {
    ChannelRegistry reg;
    reg.Register(
        std::make_unique<MockChannel>("foo"));
    reg.Register(
        std::make_unique<MockChannel>("bar"));

    auto* foo = reg.Get("foo");
    ASSERT_NE(foo, nullptr);
    EXPECT_EQ(foo->GetName(), "foo");

    auto* baz = reg.Get("baz");
    EXPECT_EQ(baz, nullptr);
}

TEST(ChannelRegistryTest, StartAllAndStopAll) {
    ChannelRegistry reg;
    reg.Register(
        std::make_unique<MockChannel>("ch1"));
    reg.Register(
        std::make_unique<MockChannel>("ch2"));

    // Before start
    EXPECT_FALSE(reg.Get("ch1")->IsRunning());
    EXPECT_FALSE(reg.Get("ch2")->IsRunning());

    reg.StartAll();
    EXPECT_TRUE(reg.Get("ch1")->IsRunning());
    EXPECT_TRUE(reg.Get("ch2")->IsRunning());

    reg.StopAll();
    EXPECT_FALSE(reg.Get("ch1")->IsRunning());
    EXPECT_FALSE(reg.Get("ch2")->IsRunning());
}

TEST(ChannelRegistryTest,
    FailedStartDoesNotBlock) {
    ChannelRegistry reg;
    reg.Register(
        std::make_unique<MockChannel>(
            "ok_ch", true));
    reg.Register(
        std::make_unique<MockChannel>(
            "fail_ch", false));
    reg.Register(
        std::make_unique<MockChannel>(
            "ok_ch2", true));

    reg.StartAll();

    // ok channels should still start
    EXPECT_TRUE(reg.Get("ok_ch")->IsRunning());
    EXPECT_FALSE(
        reg.Get("fail_ch")->IsRunning());
    EXPECT_TRUE(reg.Get("ok_ch2")->IsRunning());
}

TEST(ChannelRegistryTest,
    NullRegistrationIgnored) {
    ChannelRegistry reg;
    reg.Register(nullptr);
    EXPECT_EQ(reg.Size(), 0u);
}
