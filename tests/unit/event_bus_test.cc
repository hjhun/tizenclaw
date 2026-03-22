#include <gtest/gtest.h>
#include <chrono>
#include <thread>
#include <atomic>
#include <vector>
#include "event_bus.hh"
#include "system_context_provider.hh"

using namespace tizenclaw;

class EventBusTest : public ::testing::Test {
 protected:
  void SetUp() override {
    EventBus::GetInstance().Start();
  }

  void TearDown() override {
    EventBus::GetInstance().Stop();
  }
};

// -------------------------------------------
// EventBus Pub/Sub Tests
// -------------------------------------------
TEST_F(EventBusTest,
       PublishAndSubscribe) {
  std::atomic<int> received{0};

  int sub_id = EventBus::GetInstance().Subscribe(
      EventType::kBatteryChanged,
      [&](const SystemEvent& event) {
        EXPECT_EQ(event.name, "battery.level_changed");
        EXPECT_EQ(event.data["level"], 42);
        received.fetch_add(1);
      });

  SystemEvent event;
  event.type = EventType::kBatteryChanged;
  event.source = "battery";
  event.name = "battery.level_changed";
  event.data = {{"level", 42}};
  event.plugin_id = "builtin";

  EventBus::GetInstance().Publish(event);

  // Wait for dispatch
  std::this_thread::sleep_for(
      std::chrono::milliseconds(200));

  EXPECT_EQ(received.load(), 1);
  EventBus::GetInstance().Unsubscribe(sub_id);
}

TEST_F(EventBusTest,
       SubscribeAllReceivesAllTypes) {
  std::atomic<int> received{0};

  int sub_id = EventBus::GetInstance().SubscribeAll(
      [&](const SystemEvent&) {
        received.fetch_add(1);
      });

  // Publish different event types
  SystemEvent e1;
  e1.type = EventType::kBatteryChanged;
  e1.source = "battery";
  e1.name = "battery.test";
  e1.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e1);

  SystemEvent e2;
  e2.type = EventType::kNetworkChanged;
  e2.source = "network";
  e2.name = "network.test";
  e2.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e2);

  SystemEvent e3;
  e3.type = EventType::kDisplayChanged;
  e3.source = "display";
  e3.name = "display.test";
  e3.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e3);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(300));

  EXPECT_EQ(received.load(), 3);
  EventBus::GetInstance().Unsubscribe(sub_id);
}

TEST_F(EventBusTest,
       TypeFilteringWorks) {
  std::atomic<int> battery_count{0};
  std::atomic<int> network_count{0};

  int bat_id = EventBus::GetInstance().Subscribe(
      EventType::kBatteryChanged,
      [&](const SystemEvent&) {
        battery_count.fetch_add(1);
      });

  int net_id = EventBus::GetInstance().Subscribe(
      EventType::kNetworkChanged,
      [&](const SystemEvent&) {
        network_count.fetch_add(1);
      });

  // Publish battery event
  SystemEvent e1;
  e1.type = EventType::kBatteryChanged;
  e1.source = "battery";
  e1.name = "battery.test";
  e1.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e1);

  // Publish network event
  SystemEvent e2;
  e2.type = EventType::kNetworkChanged;
  e2.source = "network";
  e2.name = "network.test";
  e2.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e2);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(300));

  EXPECT_EQ(battery_count.load(), 1);
  EXPECT_EQ(network_count.load(), 1);

  EventBus::GetInstance().Unsubscribe(bat_id);
  EventBus::GetInstance().Unsubscribe(net_id);
}

TEST_F(EventBusTest,
       UnsubscribeStopsDelivery) {
  std::atomic<int> received{0};

  int sub_id = EventBus::GetInstance().SubscribeAll(
      [&](const SystemEvent&) {
        received.fetch_add(1);
      });

  SystemEvent e1;
  e1.type = EventType::kBatteryChanged;
  e1.source = "battery";
  e1.name = "test";
  e1.plugin_id = "builtin";
  EventBus::GetInstance().Publish(e1);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(200));
  EXPECT_EQ(received.load(), 1);

  // Unsubscribe
  EventBus::GetInstance().Unsubscribe(sub_id);

  EventBus::GetInstance().Publish(e1);
  std::this_thread::sleep_for(
      std::chrono::milliseconds(200));

  // Should still be 1
  EXPECT_EQ(received.load(), 1);
}

TEST_F(EventBusTest,
       EventTimestampAutoSet) {
  std::atomic<bool> checked{false};

  int sub_id = EventBus::GetInstance().SubscribeAll(
      [&](const SystemEvent& event) {
        EXPECT_GT(event.timestamp, 0);
        checked.store(true);
      });

  SystemEvent event;
  event.type = EventType::kCustom;
  event.source = "test";
  event.name = "test.auto_timestamp";
  event.plugin_id = "test";
  event.timestamp = 0;  // Should be auto-set

  EventBus::GetInstance().Publish(event);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(200));

  EXPECT_TRUE(checked.load());
  EventBus::GetInstance().Unsubscribe(sub_id);
}

// -------------------------------------------
// EventSource Registration Tests
// -------------------------------------------
TEST_F(EventBusTest,
       RegisterAndListSources) {
  EventSourceDescriptor desc;
  desc.name = "test_source";
  desc.plugin_id = "org.test.plugin";
  desc.type = "event_source";
  desc.version = "1.0";
  desc.collect_method = "poll";
  desc.poll_interval_sec = 10;

  EventBus::GetInstance().RegisterEventSource(desc);

  auto sources =
      EventBus::GetInstance().ListEventSources();
  bool found = false;
  for (const auto& s : sources) {
    if (s.name == "test_source") {
      found = true;
      EXPECT_EQ(s.plugin_id, "org.test.plugin");
      EXPECT_EQ(s.collect_method, "poll");
    }
  }
  EXPECT_TRUE(found);

  // Unregister
  EventBus::GetInstance().UnregisterEventSource(
      "test_source");
  sources = EventBus::GetInstance().ListEventSources();
  found = false;
  for (const auto& s : sources) {
    if (s.name == "test_source") found = true;
  }
  EXPECT_FALSE(found);
}

// -------------------------------------------
// SystemContextProvider Tests
// -------------------------------------------
TEST_F(EventBusTest,
       SystemContextProviderUpdatesOnEvent) {
  SystemContextProvider provider;
  provider.Start();

  // Publish battery event
  SystemEvent event;
  event.type = EventType::kBatteryChanged;
  event.source = "battery";
  event.name = "battery.level_changed";
  event.data = {{"level", 75}, {"charging", true}};
  event.plugin_id = "builtin";
  EventBus::GetInstance().Publish(event);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(300));

  auto ctx = provider.GetContextJson();
  EXPECT_EQ(ctx["runtime"]["battery"]["level"], 75);
  EXPECT_EQ(ctx["runtime"]["battery"]["charging"],
            true);

  // Check recent events
  EXPECT_FALSE(ctx["recent_events"].empty());
  auto last_event = ctx["recent_events"].back();
  EXPECT_EQ(last_event["event"],
            "battery.level_changed");

  provider.Stop();
}

TEST_F(EventBusTest,
       SystemContextProviderNetworkUpdate) {
  SystemContextProvider provider;
  provider.Start();

  SystemEvent event;
  event.type = EventType::kNetworkChanged;
  event.source = "network";
  event.name = "network.connected";
  event.data = {{"status", "connected"},
                {"type", "wifi"}};
  event.plugin_id = "builtin";
  EventBus::GetInstance().Publish(event);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(300));

  auto ctx = provider.GetContextJson();
  EXPECT_EQ(ctx["runtime"]["network"], "connected");

  provider.Stop();
}

TEST_F(EventBusTest,
       SystemContextStringNotEmpty) {
  SystemContextProvider provider;
  provider.Start();

  SystemEvent event;
  event.type = EventType::kBatteryChanged;
  event.source = "battery";
  event.name = "battery.test";
  event.data = {{"level", 50}};
  event.plugin_id = "builtin";
  EventBus::GetInstance().Publish(event);

  std::this_thread::sleep_for(
      std::chrono::milliseconds(300));

  auto ctx_str = provider.GetContextString();
  EXPECT_FALSE(ctx_str.empty());

  // Should be valid JSON
  auto parsed = nlohmann::json::parse(ctx_str);
  EXPECT_TRUE(parsed.contains("device"));
  EXPECT_TRUE(parsed.contains("runtime"));
  EXPECT_TRUE(parsed.contains("recent_events"));

  provider.Stop();
}
