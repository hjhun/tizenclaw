#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <sys/stat.h>

#include "session_store.hh"

class SessionStoreTest : public ::testing::Test {
protected:
    void SetUp() override {
        test_dir_ = "/tmp/tizenclaw_test_sessions";
        mkdir(test_dir_.c_str(), 0700);
        store_.SetDirectory(test_dir_);
    }

    void TearDown() override {
        // Clean up test files
        system(("rm -rf " + test_dir_).c_str());
    }

    std::string test_dir_;
    SessionStore store_;
};

TEST_F(SessionStoreTest, SaveAndLoadSession) {
    std::vector<LlmMessage> history;

    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = "Hello world";
    history.push_back(user_msg);

    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = "Hi there!";
    history.push_back(assistant_msg);

    EXPECT_TRUE(store_.SaveSession("test1", history));

    auto loaded = store_.LoadSession("test1");
    ASSERT_EQ(loaded.size(), 2u);
    EXPECT_EQ(loaded[0].role, "user");
    EXPECT_EQ(loaded[0].text, "Hello world");
    EXPECT_EQ(loaded[1].role, "assistant");
    EXPECT_EQ(loaded[1].text, "Hi there!");
}

TEST_F(SessionStoreTest, SaveWithToolCalls) {
    std::vector<LlmMessage> history;

    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    LlmToolCall tc;
    tc.id = "call_abc123";
    tc.name = "list_apps";
    tc.args = {{"count", 5}};
    assistant_msg.tool_calls.push_back(tc);
    history.push_back(assistant_msg);

    LlmMessage tool_msg;
    tool_msg.role = "tool";
    tool_msg.tool_name = "list_apps";
    tool_msg.tool_call_id = "call_abc123";
    tool_msg.tool_result = {{"apps", {"app1"}}};
    history.push_back(tool_msg);

    EXPECT_TRUE(store_.SaveSession("test2", history));

    auto loaded = store_.LoadSession("test2");
    ASSERT_EQ(loaded.size(), 2u);
    EXPECT_EQ(loaded[0].tool_calls.size(), 1u);
    EXPECT_EQ(loaded[0].tool_calls[0].id,
              "call_abc123");
    EXPECT_EQ(loaded[0].tool_calls[0].name,
              "list_apps");
    EXPECT_EQ(loaded[1].tool_call_id,
              "call_abc123");
    EXPECT_EQ(loaded[1].tool_name, "list_apps");
}

TEST_F(SessionStoreTest, LoadNonExistentSession) {
    auto loaded = store_.LoadSession("nonexistent");
    EXPECT_TRUE(loaded.empty());
}

TEST_F(SessionStoreTest, SaveEmptySession) {
    std::vector<LlmMessage> empty;
    EXPECT_FALSE(store_.SaveSession("empty", empty));
}

TEST_F(SessionStoreTest, DeleteSession) {
    std::vector<LlmMessage> history;
    LlmMessage msg;
    msg.role = "user";
    msg.text = "test";
    history.push_back(msg);

    store_.SaveSession("del_test", history);
    auto loaded = store_.LoadSession("del_test");
    EXPECT_FALSE(loaded.empty());

    store_.DeleteSession("del_test");
    loaded = store_.LoadSession("del_test");
    EXPECT_TRUE(loaded.empty());
}
