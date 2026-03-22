#include <gtest/gtest.h>
#include "tool_dispatcher.hh"

using namespace tizenclaw;

class ToolDispatcherTest : public ::testing::Test {
 protected:
  ToolDispatcher dispatcher;
};

TEST_F(ToolDispatcherTest, RegisterAndExecute) {
  dispatcher.Register("echo",
      [](const nlohmann::json& args,
         const std::string&,
         const std::string&) {
        return args.value("message", "");
      });

  auto result = dispatcher.Execute(
      "echo", {{"message", "hello"}}, "session1");
  EXPECT_EQ(result, "hello");
}

TEST_F(ToolDispatcherTest, ExecuteUnknownTool) {
  auto result = dispatcher.Execute(
      "nonexistent", {}, "session1");
  EXPECT_NE(result.find("error"),
            std::string::npos);
}

TEST_F(ToolDispatcherTest, HasTool) {
  EXPECT_FALSE(dispatcher.HasTool("echo"));

  dispatcher.Register("echo",
      [](const nlohmann::json&,
         const std::string&,
         const std::string&) {
        return std::string("ok");
      });

  EXPECT_TRUE(dispatcher.HasTool("echo"));
}

TEST_F(ToolDispatcherTest, Unregister) {
  dispatcher.Register("temp",
      [](const nlohmann::json&,
         const std::string&,
         const std::string&) {
        return std::string("ok");
      });
  EXPECT_TRUE(dispatcher.HasTool("temp"));

  dispatcher.Unregister("temp");
  EXPECT_FALSE(dispatcher.HasTool("temp"));
}

TEST_F(ToolDispatcherTest, ListTools) {
  dispatcher.Register("a",
      [](const nlohmann::json&,
         const std::string&,
         const std::string&) {
        return std::string("");
      });
  dispatcher.Register("b",
      [](const nlohmann::json&,
         const std::string&,
         const std::string&) {
        return std::string("");
      });

  auto tools = dispatcher.ListTools();
  EXPECT_EQ(tools.size(), 2u);
}

TEST_F(ToolDispatcherTest, Size) {
  EXPECT_EQ(dispatcher.Size(), 0u);

  dispatcher.Register("x",
      [](const nlohmann::json&,
         const std::string&,
         const std::string&) {
        return std::string("");
      });
  EXPECT_EQ(dispatcher.Size(), 1u);
}

TEST_F(ToolDispatcherTest,
       ToolReceivesCorrectArgs) {
  std::string received_name;
  std::string received_session;

  dispatcher.Register("test",
      [&](const nlohmann::json& args,
          const std::string& name,
          const std::string& sid) {
        received_name = name;
        received_session = sid;
        return args.dump();
      });

  auto result = dispatcher.Execute(
      "test", {{"key", "value"}}, "my_session");
  EXPECT_EQ(received_name, "test");
  EXPECT_EQ(received_session, "my_session");
  EXPECT_NE(result.find("key"), std::string::npos);
}
