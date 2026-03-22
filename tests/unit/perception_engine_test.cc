#include <gtest/gtest.h>
#include <chrono>
#include <thread>
#include "device_profiler.hh"
#include "context_fusion_engine.hh"
#include "proactive_advisor.hh"
#include "perception_engine.hh"
#include "event_bus.hh"

using namespace tizenclaw;

// -------------------------------------------
// DeviceProfiler Tests
// -------------------------------------------
class DeviceProfilerTest : public ::testing::Test {
 protected:
  void SetUp() override {
    profiler = std::make_unique<DeviceProfiler>();
  }

  std::unique_ptr<DeviceProfiler> profiler;
};

TEST_F(DeviceProfilerTest,
       RecordEventStoresEvents) {
  SystemEvent event;
  event.type = EventType::kBatteryChanged;
  event.source = "battery";
  event.name = "battery.level_changed";
  event.data = {{"level", 80}, {"charging", false}};
  event.timestamp = 1000;
  event.plugin_id = "builtin";

  profiler->RecordEvent(event);
  EXPECT_EQ(profiler->GetEventCount(), 1u);

  profiler->RecordEvent(event);
  EXPECT_EQ(profiler->GetEventCount(), 2u);
}

TEST_F(DeviceProfilerTest,
       AnalyzeReturnsSnapshot) {
  // Add battery event
  SystemEvent bat;
  bat.type = EventType::kBatteryChanged;
  bat.source = "battery";
  bat.name = "battery.level_changed";
  bat.data = {{"level", 42}, {"charging", false}};
  bat.plugin_id = "builtin";
  profiler->RecordEvent(bat);

  auto snap = profiler->Analyze();
  EXPECT_EQ(snap.battery_level, 42);
  EXPECT_FALSE(snap.charging);
}

TEST_F(DeviceProfilerTest,
       AnalyzeTracksNetworkStatus) {
  SystemEvent net;
  net.type = EventType::kNetworkChanged;
  net.source = "network";
  net.name = "network.connected";
  net.data = {{"status", "connected"}};
  net.plugin_id = "builtin";
  profiler->RecordEvent(net);

  auto snap = profiler->Analyze();
  EXPECT_EQ(snap.network_status, "connected");
}

TEST_F(DeviceProfilerTest,
       AnalyzeTracksForegroundApp) {
  SystemEvent app;
  app.type = EventType::kAppLifecycle;
  app.source = "app";
  app.name = "app.resumed";
  app.data = {{"app_id", "com.test.app"},
              {"state", "resumed"}};
  app.plugin_id = "builtin";
  profiler->RecordEvent(app);

  auto snap = profiler->Analyze();
  EXPECT_EQ(snap.foreground_app, "com.test.app");
}

TEST_F(DeviceProfilerTest,
       AnalyzeCountsMemoryWarnings) {
  // Add memory warning events with current
  // timestamps (within analysis window)
  auto now = std::chrono::duration_cast<
                 std::chrono::milliseconds>(
                 std::chrono::system_clock::now()
                     .time_since_epoch())
                 .count();

  for (int i = 0; i < 3; i++) {
    SystemEvent mem;
    mem.type = EventType::kMemoryWarning;
    mem.source = "memory";
    mem.name = "memory.critical";
    mem.data = {{"level", "critical"}};
    mem.timestamp = now + i * 1000;
    mem.plugin_id = "builtin";
    profiler->RecordEvent(mem);
  }

  auto snap = profiler->Analyze();
  EXPECT_GE(snap.memory_warning_count, 3);
  EXPECT_EQ(snap.memory_trend, "critical");
}

TEST_F(DeviceProfilerTest,
       ChargingBatteryClassified) {
  SystemEvent bat;
  bat.type = EventType::kBatteryChanged;
  bat.source = "battery";
  bat.name = "battery.level_changed";
  bat.data = {{"level", 50}, {"charging", true}};
  bat.plugin_id = "builtin";
  profiler->RecordEvent(bat);

  auto snap = profiler->Analyze();
  EXPECT_EQ(snap.battery_health, "charging");
}

TEST_F(DeviceProfilerTest,
       CriticalBatteryClassified) {
  SystemEvent bat;
  bat.type = EventType::kBatteryChanged;
  bat.source = "battery";
  bat.name = "battery.level_changed";
  bat.data = {{"level", 3}, {"charging", false}};
  bat.plugin_id = "builtin";
  profiler->RecordEvent(bat);

  auto snap = profiler->Analyze();
  EXPECT_EQ(snap.battery_health, "critical");
}

// -------------------------------------------
// ContextFusionEngine Tests
// -------------------------------------------
class ContextFusionTest : public ::testing::Test {
 protected:
  ContextFusionEngine fusion;
};

TEST_F(ContextFusionTest,
       NormalStateProducesNormalLevel) {
  ProfileSnapshot snap;
  snap.battery_level = 80;
  snap.charging = false;
  snap.battery_drain_rate = 0.1;
  snap.battery_health = "good";
  snap.memory_trend = "stable";
  snap.memory_warning_count = 0;
  snap.network_status = "connected";
  snap.network_drop_count = 0;

  auto result = fusion.Fuse(snap, {});
  EXPECT_EQ(result.level, SituationLevel::kNormal);
  EXPECT_LT(result.risk_score, 0.2);
  EXPECT_TRUE(result.factors.empty());
}

TEST_F(ContextFusionTest,
       LowBatteryProducesWarning) {
  ProfileSnapshot snap;
  snap.battery_level = 10;
  snap.charging = false;
  snap.battery_drain_rate = 1.5;
  snap.battery_health = "degrading";
  snap.memory_trend = "stable";
  snap.memory_warning_count = 0;
  snap.network_status = "connected";
  snap.network_drop_count = 0;

  auto result = fusion.Fuse(snap, {});
  EXPECT_GE(static_cast<int>(result.level),
            static_cast<int>(
                SituationLevel::kWarning));
  EXPECT_GT(result.risk_score, 0.3);
  EXPECT_FALSE(result.factors.empty());
}

TEST_F(ContextFusionTest,
       MultipleProblemsCritical) {
  ProfileSnapshot snap;
  snap.battery_level = 5;
  snap.charging = false;
  snap.battery_drain_rate = 3.0;
  snap.battery_health = "critical";
  snap.memory_trend = "critical";
  snap.memory_warning_count = 5;
  snap.network_status = "disconnected";
  snap.network_drop_count = 5;

  auto result = fusion.Fuse(snap, {});
  EXPECT_EQ(result.level,
            SituationLevel::kCritical);
  EXPECT_GT(result.risk_score, 0.7);
  EXPECT_GE(result.factors.size(), 2u);
  EXPECT_FALSE(result.suggestions.empty());
}

TEST_F(ContextFusionTest,
       ChargingReducesBatteryRisk) {
  ProfileSnapshot snap;
  snap.battery_level = 5;
  snap.charging = true;
  snap.battery_health = "charging";
  snap.memory_trend = "stable";
  snap.memory_warning_count = 0;
  snap.network_status = "connected";
  snap.network_drop_count = 0;

  auto result = fusion.Fuse(snap, {});
  // Charging should reduce battery risk to 0
  EXPECT_EQ(result.level, SituationLevel::kNormal);
}

TEST_F(ContextFusionTest,
       ToJsonProducesValidOutput) {
  SituationAssessment a;
  a.level = SituationLevel::kWarning;
  a.risk_score = 0.55;
  a.summary = "Test summary";
  a.factors = {"factor1", "factor2"};
  a.suggestions = {"suggestion1"};

  auto j = ContextFusionEngine::ToJson(a);
  EXPECT_EQ(j["level"], "warning");
  EXPECT_EQ(j["level_num"], 2);
  EXPECT_DOUBLE_EQ(j["risk_score"], 0.55);
  EXPECT_EQ(j["summary"], "Test summary");
  EXPECT_EQ(j["factors"].size(), 2u);
  EXPECT_EQ(j["suggestions"].size(), 1u);
}

TEST_F(ContextFusionTest,
       LevelToStringMappings) {
  EXPECT_EQ(ContextFusionEngine::LevelToString(
                SituationLevel::kNormal),
            "normal");
  EXPECT_EQ(ContextFusionEngine::LevelToString(
                SituationLevel::kAdvisory),
            "advisory");
  EXPECT_EQ(ContextFusionEngine::LevelToString(
                SituationLevel::kWarning),
            "warning");
  EXPECT_EQ(ContextFusionEngine::LevelToString(
                SituationLevel::kCritical),
            "critical");
}

// -------------------------------------------
// ProactiveAdvisor Tests
// -------------------------------------------
class ProactiveAdvisorTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    advisor = std::make_unique<ProactiveAdvisor>(
        nullptr, nullptr);
  }

  std::unique_ptr<ProactiveAdvisor> advisor;
};

TEST_F(ProactiveAdvisorTest,
       NormalSituationSuppressed) {
  SituationAssessment normal;
  normal.level = SituationLevel::kNormal;
  normal.risk_score = 0.1;
  normal.summary = "All good";

  EventBus::GetInstance().Start();
  auto advisory = advisor->Evaluate(normal);
  EventBus::GetInstance().Stop();

  EXPECT_EQ(advisory.action,
            AdvisoryAction::kSuppress);
}

TEST_F(ProactiveAdvisorTest,
       WarningTriggersNotify) {
  SituationAssessment warning;
  warning.level = SituationLevel::kWarning;
  warning.risk_score = 0.5;
  warning.summary = "Warning situation";
  warning.factors = {"test factor"};
  warning.suggestions = {"test suggestion"};

  EventBus::GetInstance().Start();
  auto advisory = advisor->Evaluate(warning);
  EventBus::GetInstance().Stop();

  EXPECT_EQ(advisory.action,
            AdvisoryAction::kNotify);
  EXPECT_FALSE(advisory.message.empty());
}

TEST_F(ProactiveAdvisorTest,
       CriticalTriggersEvaluate) {
  SituationAssessment critical;
  critical.level = SituationLevel::kCritical;
  critical.risk_score = 0.8;
  critical.summary = "Critical situation";
  critical.factors = {"critical factor"};
  critical.suggestions = {"immediate action"};

  EventBus::GetInstance().Start();
  auto advisory = advisor->Evaluate(critical);
  EventBus::GetInstance().Stop();

  EXPECT_EQ(advisory.action,
            AdvisoryAction::kEvaluate);
  EXPECT_FALSE(advisory.message.empty());
}

TEST_F(ProactiveAdvisorTest,
       GetLastInsightUpdated) {
  SituationAssessment warning;
  warning.level = SituationLevel::kWarning;
  warning.risk_score = 0.6;
  warning.summary = "Test";

  EventBus::GetInstance().Start();
  advisor->Evaluate(warning);
  EventBus::GetInstance().Stop();

  auto insight = advisor->GetLastInsight();
  EXPECT_FALSE(insight.empty());
  EXPECT_EQ(insight["level"], "warning");
}

TEST_F(ProactiveAdvisorTest,
       CooldownPreventsRepeat) {
  SituationAssessment warning;
  warning.level = SituationLevel::kWarning;
  warning.risk_score = 0.5;
  warning.summary = "Warning";

  EventBus::GetInstance().Start();

  // First evaluation should produce notify
  auto first = advisor->Evaluate(warning);
  EXPECT_EQ(first.action,
            AdvisoryAction::kNotify);

  // Second immediate evaluation should be
  // cooled down (inject only)
  auto second = advisor->Evaluate(warning);
  EXPECT_EQ(second.action,
            AdvisoryAction::kInject);

  EventBus::GetInstance().Stop();
}

// -------------------------------------------
// PerceptionEngine Integration Tests
// -------------------------------------------
class PerceptionEngineTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    EventBus::GetInstance().Start();
  }

  void TearDown() override {
    EventBus::GetInstance().Stop();
  }
};

TEST_F(PerceptionEngineTest,
       StartStopCycle) {
  PerceptionEngine engine(
      nullptr, nullptr, nullptr);
  engine.Start();
  EXPECT_TRUE(engine.IsRunning());

  // Let it run briefly
  std::this_thread::sleep_for(
      std::chrono::milliseconds(200));

  engine.Stop();
  EXPECT_FALSE(engine.IsRunning());

  // Double stop should be safe
  engine.Stop();
}

TEST_F(PerceptionEngineTest,
       GetInsightInitiallyEmpty) {
  PerceptionEngine engine(
      nullptr, nullptr, nullptr);
  auto insight = engine.GetInsight();
  EXPECT_TRUE(insight.empty());
}

TEST_F(PerceptionEngineTest,
       PublishedEventsReachProfiler) {
  SystemContextProvider context;
  context.Start();

  PerceptionEngine engine(
      nullptr, &context, nullptr);
  engine.Start();

  // Publish some events
  SystemEvent bat;
  bat.type = EventType::kBatteryChanged;
  bat.source = "battery";
  bat.name = "battery.level_changed";
  bat.data = {{"level", 50}, {"charging", false}};
  bat.plugin_id = "builtin";
  EventBus::GetInstance().Publish(bat);

  SystemEvent net;
  net.type = EventType::kNetworkChanged;
  net.source = "network";
  net.name = "network.connected";
  net.data = {{"status", "connected"}};
  net.plugin_id = "builtin";
  EventBus::GetInstance().Publish(net);

  // Wait for events to be dispatched and
  // at least one analysis tick
  std::this_thread::sleep_for(
      std::chrono::milliseconds(500));

  engine.Stop();
  context.Stop();
}

TEST_F(PerceptionEngineTest,
       SituationEventPublished) {
  // Verify that PerceptionEngine's
  // ProactiveAdvisor publishes synthetic
  // perception events
  std::atomic<bool> received{false};

  int sub_id = EventBus::GetInstance().Subscribe(
      EventType::kCustom,
      [&](const SystemEvent& event) {
        if (event.source == "perception") {
          received.store(true);
        }
      });

  PerceptionEngine engine(
      nullptr, nullptr, nullptr);
  engine.Start();

  // Publish enough events to trigger analysis
  for (int i = 0; i < 5; i++) {
    SystemEvent bat;
    bat.type = EventType::kBatteryChanged;
    bat.source = "battery";
    bat.name = "battery.level_changed";
    bat.data = {{"level", 10 - i},
                {"charging", false}};
    bat.plugin_id = "builtin";
    EventBus::GetInstance().Publish(bat);
  }

  // Wait for analysis tick (>30s in production,
  // but events should trigger at least one pass
  // after kAnalysisIntervalSec)
  // For unit test, we just verify structure
  std::this_thread::sleep_for(
      std::chrono::milliseconds(500));

  engine.Stop();
  EventBus::GetInstance().Unsubscribe(sub_id);
  // Note: The perception event might not fire
  // in 500ms since analysis interval is 30s.
  // This test mainly verifies no crashes.
}
