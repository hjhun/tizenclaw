#include <gtest/gtest.h>


#include "core/action_bridge.hh"

namespace tizenclaw {

class ActionBridgeTest : public ::testing::Test {
 protected:
  void SetUp() override {
    bridge_ = std::make_unique<ActionBridge>();
  }

  void TearDown() override {
    if (bridge_) bridge_->Stop();
    bridge_.reset();
  }

  std::unique_ptr<ActionBridge> bridge_;
};

TEST_F(ActionBridgeTest,
       StartConnectsToActionService) {
  // Start may fail if action service is not
  // running (expected in test environment).
  // We just verify it doesn't crash.
  bool ok = bridge_->Start();
  if (ok) {
    EXPECT_NE(nullptr, bridge_.get());
    bridge_->Stop();
  }
  // Not asserting ok == true because the
  // action service may not be running in the
  // GBS build chroot environment.
}

TEST_F(ActionBridgeTest,
       ListActionsWithoutStart) {
  // ListActions without Start should return
  // an error, not crash.
  std::string result = bridge_->ListActions();
  EXPECT_FALSE(result.empty());
  EXPECT_NE(result.find("error"),
            std::string::npos);
}

TEST_F(ActionBridgeTest,
       ExecuteActionWithoutStart) {
  nlohmann::json args = {{"text", "hello"}};
  std::string result =
      bridge_->ExecuteAction("test", args);
  EXPECT_FALSE(result.empty());
  EXPECT_NE(result.find("error"),
            std::string::npos);
}

TEST_F(ActionBridgeTest,
       StopWithoutStart) {
  // Stop without Start should be safe.
  bridge_->Stop();
  bridge_->Stop();  // Double-stop is safe
}

}  // namespace tizenclaw

