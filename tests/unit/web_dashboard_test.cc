#include <gtest/gtest.h>

#include "web_dashboard.hh"

using namespace tizenclaw;

class WebDashboardTest : public ::testing::Test {
protected:
  void SetUp() override {
    dashboard_ =
        new WebDashboard(nullptr, nullptr);
  }

  void TearDown() override {
    delete dashboard_;
    dashboard_ = nullptr;
  }

  WebDashboard* dashboard_;
};

TEST_F(WebDashboardTest, GetName) {
  EXPECT_EQ(dashboard_->GetName(),
            "web_dashboard");
}

TEST_F(WebDashboardTest, InitialState) {
  EXPECT_FALSE(dashboard_->IsRunning());
}

TEST_F(WebDashboardTest,
       StartFailsWithoutWebRoot) {
  // Without web root directory, Start should
  // fail gracefully
  EXPECT_FALSE(dashboard_->Start());
  EXPECT_FALSE(dashboard_->IsRunning());
}

TEST_F(WebDashboardTest, StopWhenNotRunning) {
  // Stop on non-running dashboard should be safe
  dashboard_->Stop();
  EXPECT_FALSE(dashboard_->IsRunning());
}
