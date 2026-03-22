#include <gtest/gtest.h>

#include "voice_channel.hh"

using namespace tizenclaw;

class VoiceChannelTest : public ::testing::Test {
protected:
  void SetUp() override {
    channel_ = new VoiceChannel(nullptr);
  }

  void TearDown() override {
    delete channel_;
    channel_ = nullptr;
  }

  VoiceChannel* channel_;
};

TEST_F(VoiceChannelTest, GetName) {
  EXPECT_EQ(channel_->GetName(), "voice");
}

TEST_F(VoiceChannelTest, InitialState) {
  EXPECT_FALSE(channel_->IsRunning());
}

TEST_F(VoiceChannelTest,
       StartWithoutSttTts) {
  // Without STT/TTS compiled in, Start should
  // return false gracefully
#if !defined(TIZEN_STT_ENABLED) && \
    !defined(TIZEN_TTS_ENABLED)
  EXPECT_FALSE(channel_->Start());
  EXPECT_FALSE(channel_->IsRunning());
#endif
}

TEST_F(VoiceChannelTest, StopWhenNotRunning) {
  channel_->Stop();
  EXPECT_FALSE(channel_->IsRunning());
}
