#include <gtest/gtest.h>
#include <json.hpp>
#include <thread>
#include <vector>
#include "health_monitor.hh"

using namespace tizenclaw;

class HealthMonitorTest
    : public ::testing::Test {
 protected:
  HealthMonitor monitor_;
};

TEST_F(HealthMonitorTest,
       InitialCountersAreZero) {
  EXPECT_EQ(monitor_.GetRequestCount(), 0u);
  EXPECT_EQ(monitor_.GetErrorCount(), 0u);
  EXPECT_EQ(monitor_.GetLlmCallCount(), 0u);
  EXPECT_EQ(monitor_.GetToolCallCount(), 0u);
}

TEST_F(HealthMonitorTest,
       IncrementRequestCount) {
  monitor_.IncrementRequestCount();
  monitor_.IncrementRequestCount();
  monitor_.IncrementRequestCount();
  EXPECT_EQ(monitor_.GetRequestCount(), 3u);
}

TEST_F(HealthMonitorTest,
       IncrementErrorCount) {
  monitor_.IncrementErrorCount();
  EXPECT_EQ(monitor_.GetErrorCount(), 1u);
}

TEST_F(HealthMonitorTest,
       IncrementLlmCallCount) {
  monitor_.IncrementLlmCallCount();
  monitor_.IncrementLlmCallCount();
  EXPECT_EQ(monitor_.GetLlmCallCount(), 2u);
}

TEST_F(HealthMonitorTest,
       IncrementToolCallCount) {
  monitor_.IncrementToolCallCount();
  EXPECT_EQ(monitor_.GetToolCallCount(), 1u);
}

TEST_F(HealthMonitorTest,
       UptimeIsPositive) {
  double uptime =
      monitor_.GetUptimeSeconds();
  EXPECT_GE(uptime, 0.0);
}

TEST_F(HealthMonitorTest,
       GetMetricsJsonReturnsValidJson) {
  monitor_.IncrementRequestCount();
  monitor_.IncrementErrorCount();
  monitor_.IncrementLlmCallCount();
  monitor_.IncrementToolCallCount();

  std::string json_str =
      monitor_.GetMetricsJson();
  auto metrics =
      nlohmann::json::parse(json_str);

  // Uptime
  EXPECT_TRUE(metrics.contains("uptime"));
  EXPECT_TRUE(
      metrics["uptime"].contains("seconds"));
  EXPECT_TRUE(
      metrics["uptime"].contains("formatted"));
  EXPECT_GE(
      metrics["uptime"]["seconds"]
          .get<double>(),
      0.0);

  // Counters
  EXPECT_TRUE(
      metrics.contains("counters"));
  EXPECT_EQ(
      metrics["counters"]["requests"]
          .get<uint64_t>(),
      1u);
  EXPECT_EQ(
      metrics["counters"]["errors"]
          .get<uint64_t>(),
      1u);
  EXPECT_EQ(
      metrics["counters"]["llm_calls"]
          .get<uint64_t>(),
      1u);
  EXPECT_EQ(
      metrics["counters"]["tool_calls"]
          .get<uint64_t>(),
      1u);

  // Memory
  EXPECT_TRUE(metrics.contains("memory"));
  EXPECT_TRUE(
      metrics["memory"].contains(
          "vm_rss_kb"));

  // CPU
  EXPECT_TRUE(metrics.contains("cpu"));
  EXPECT_TRUE(
      metrics["cpu"].contains("load_1m"));

  // Threads / PID
  EXPECT_TRUE(metrics.contains("threads"));
  EXPECT_TRUE(metrics.contains("pid"));
  EXPECT_GT(
      metrics["pid"].get<int>(), 0);
}

TEST_F(HealthMonitorTest,
       UptimeFormattedString) {
  auto metrics = nlohmann::json::parse(
      monitor_.GetMetricsJson());
  std::string fmt =
      metrics["uptime"]["formatted"]
          .get<std::string>();
  // Should contain h/m/s
  EXPECT_NE(fmt.find("h"), std::string::npos);
  EXPECT_NE(fmt.find("m"), std::string::npos);
  EXPECT_NE(fmt.find("s"), std::string::npos);
}

TEST_F(HealthMonitorTest,
       ConcurrentIncrements) {
  const int count = 1000;
  std::vector<std::thread> threads;
  for (int i = 0; i < 4; ++i) {
    threads.emplace_back([&]() {
      for (int j = 0; j < count; ++j) {
        monitor_.IncrementRequestCount();
      }
    });
  }
  for (auto& t : threads) t.join();
  EXPECT_EQ(
      monitor_.GetRequestCount(),
      static_cast<uint64_t>(4 * count));
}
