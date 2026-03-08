#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <sys/stat.h>
#include <dirent.h>

#include "session_store.hh"

using namespace tizenclaw;


class SessionStoreTest : public ::testing::Test {
protected:
    void SetUp() override {
        base_dir_ =
            "/tmp/tizenclaw_test_store";
        test_dir_ = base_dir_ + "/sessions";
        mkdir(base_dir_.c_str(), 0700);
        mkdir(test_dir_.c_str(), 0700);
        store_.SetDirectory(test_dir_);
    }

    void TearDown() override {
        // Clean up all test dirs
        int ret = system((
            "rm -rf " + base_dir_).c_str());
        (void)ret;
    }

    std::string base_dir_;
    std::string test_dir_;
    SessionStore store_;
};

TEST_F(SessionStoreTest,
    SaveAndLoadMarkdownSession) {
    std::vector<LlmMessage> history;

    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = "Hello world";
    history.push_back(user_msg);

    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = "Hi there!";
    history.push_back(assistant_msg);

    EXPECT_TRUE(
        store_.SaveSession("test1", history));

    // Verify file exists with date prefix
    // Pattern: YYYY-MM-DD-test1.md
    bool found = false;
    std::string content;
    DIR* dir = opendir(test_dir_.c_str());
    if (dir) {
      struct dirent* ent;
      while ((ent = readdir(dir)) != nullptr) {
        std::string name(ent->d_name);
        if (name.find("-test1.md") !=
            std::string::npos) {
          found = true;
          std::ifstream check(
              test_dir_ + "/" + name);
          content.assign(
              (std::istreambuf_iterator<char>(
                  check)),
              std::istreambuf_iterator<char>());
          check.close();
          break;
        }
      }
      closedir(dir);
    }
    EXPECT_TRUE(found);

    // Should contain YAML frontmatter
    EXPECT_TRUE(
        content.find("---") != std::string::npos);
    EXPECT_TRUE(
        content.find("message_count: 2")
        != std::string::npos);
    // Should contain role headers
    EXPECT_TRUE(
        content.find("## user")
        != std::string::npos);
    EXPECT_TRUE(
        content.find("## assistant")
        != std::string::npos);

    // Load and verify
    auto loaded = store_.LoadSession("test1");
    ASSERT_EQ(loaded.size(), 2u);
    EXPECT_EQ(loaded[0].role, "user");
    EXPECT_EQ(loaded[0].text, "Hello world");
    EXPECT_EQ(loaded[1].role, "assistant");
    EXPECT_EQ(loaded[1].text, "Hi there!");
}

TEST_F(SessionStoreTest,
    MarkdownWithToolCalls) {
    std::vector<LlmMessage> history;

    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = "Let me check.";
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

    EXPECT_TRUE(
        store_.SaveSession("test2", history));

    auto loaded = store_.LoadSession("test2");
    ASSERT_EQ(loaded.size(), 2u);

    // Verify assistant message with tool calls
    EXPECT_EQ(loaded[0].role, "assistant");
    EXPECT_EQ(loaded[0].text, "Let me check.");
    ASSERT_EQ(
        loaded[0].tool_calls.size(), 1u);
    EXPECT_EQ(
        loaded[0].tool_calls[0].id,
        "call_abc123");
    EXPECT_EQ(
        loaded[0].tool_calls[0].name,
        "list_apps");

    // Verify tool result
    EXPECT_EQ(loaded[1].role, "tool");
    EXPECT_EQ(
        loaded[1].tool_call_id, "call_abc123");
    EXPECT_EQ(
        loaded[1].tool_name, "list_apps");
}

TEST_F(SessionStoreTest,
    MarkdownWithCompressedTurn) {
    std::vector<LlmMessage> history;

    // Simulate compressed message
    LlmMessage compressed;
    compressed.role = "assistant";
    compressed.text =
        "[compressed] User asked about weather.";
    history.push_back(compressed);

    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = "What about tomorrow?";
    history.push_back(user_msg);

    EXPECT_TRUE(
        store_.SaveSession("test3", history));

    auto loaded = store_.LoadSession("test3");
    ASSERT_EQ(loaded.size(), 2u);
    EXPECT_TRUE(loaded[0].text.find(
        "[compressed]") != std::string::npos);
}

TEST_F(SessionStoreTest,
    LoadNonExistentSession) {
    auto loaded =
        store_.LoadSession("nonexistent");
    EXPECT_TRUE(loaded.empty());
}

TEST_F(SessionStoreTest, SaveEmptySession) {
    std::vector<LlmMessage> empty;
    EXPECT_FALSE(
        store_.SaveSession("empty", empty));
}

TEST_F(SessionStoreTest, DeleteSession) {
    std::vector<LlmMessage> history;
    LlmMessage msg;
    msg.role = "user";
    msg.text = "test";
    history.push_back(msg);

    store_.SaveSession("del_test", history);
    auto loaded =
        store_.LoadSession("del_test");
    EXPECT_FALSE(loaded.empty());

    store_.DeleteSession("del_test");
    loaded = store_.LoadSession("del_test");
    EXPECT_TRUE(loaded.empty());
}

TEST_F(SessionStoreTest,
    JsonToMarkdownMigration) {
    // Write legacy JSON session file manually
    std::string json_path =
        test_dir_ + "/migrate_test.json";
    nlohmann::json arr = nlohmann::json::array();
    arr.push_back({
        {"role", "user"},
        {"text", "Old JSON message"}
    });
    arr.push_back({
        {"role", "assistant"},
        {"text", "Old JSON response"}
    });

    std::ofstream out(json_path);
    out << arr.dump(2);
    out.close();

    // Load should auto-migrate
    auto loaded =
        store_.LoadSession("migrate_test");
    ASSERT_EQ(loaded.size(), 2u);
    EXPECT_EQ(loaded[0].role, "user");
    EXPECT_EQ(loaded[0].text,
              "Old JSON message");

    // After migration, date-prefixed .md should
    // exist
    bool found_md = false;
    DIR* dir = opendir(test_dir_.c_str());
    if (dir) {
      struct dirent* ent;
      while ((ent = readdir(dir)) != nullptr) {
        std::string name(ent->d_name);
        if (name.find("-migrate_test.md") !=
            std::string::npos) {
          found_md = true;
          break;
        }
      }
      closedir(dir);
    }
    EXPECT_TRUE(found_md);

    // And .json should be deleted
    std::ifstream json_check(json_path);
    EXPECT_FALSE(json_check.is_open());
}

TEST_F(SessionStoreTest,
    LogAndLoadTokenUsage) {
    store_.LogTokenUsage(
        "usage_test", "gemini", 100, 50);
    store_.LogTokenUsage(
        "usage_test", "gemini", 200, 80);

    auto summary =
        store_.LoadTokenUsage("usage_test");
    EXPECT_EQ(
        summary.total_prompt_tokens, 300);
    EXPECT_EQ(
        summary.total_completion_tokens, 130);
    EXPECT_EQ(summary.entries.size(), 2u);
}

TEST_F(SessionStoreTest,
    DailyUsageAggregation) {
    // Log multiple calls from diff sessions
    store_.LogTokenUsage(
        "sess_a", "gemini", 100, 50);
    store_.LogTokenUsage(
        "sess_b", "openai", 200, 80);
    store_.LogTokenUsage(
        "sess_a", "gemini", 150, 60);

    // Get today's date string
    auto now = std::chrono::system_clock::now();
    auto t =
        std::chrono::system_clock::to_time_t(now);
    struct tm tm_buf;
    localtime_r(&t, &tm_buf);
    char date_str[16];
    strftime(date_str, sizeof(date_str),
             "%Y-%m-%d", &tm_buf);

    auto daily =
        store_.LoadDailyUsage(date_str);
    EXPECT_EQ(daily.total_prompt_tokens, 450);
    EXPECT_EQ(
        daily.total_completion_tokens, 190);
    EXPECT_EQ(daily.total_requests, 3);
}

TEST_F(SessionStoreTest,
    MonthlyUsageAggregation) {
    store_.LogTokenUsage(
        "sess_m", "gemini", 500, 200);
    store_.LogTokenUsage(
        "sess_m", "openai", 300, 100);

    auto now = std::chrono::system_clock::now();
    auto t =
        std::chrono::system_clock::to_time_t(now);
    struct tm tm_buf;
    localtime_r(&t, &tm_buf);
    char month_str[16];
    strftime(month_str, sizeof(month_str),
             "%Y-%m", &tm_buf);

    auto monthly =
        store_.LoadMonthlyUsage(month_str);
    EXPECT_EQ(monthly.total_prompt_tokens, 800);
    EXPECT_EQ(
        monthly.total_completion_tokens, 300);
    EXPECT_EQ(monthly.total_requests, 2);
}

TEST_F(SessionStoreTest,
    SanitizeRemovesOrphanedTools) {
    std::vector<LlmMessage> history;

    // Assistant message with [compressed] text
    // (tool_calls lost after compaction)
    LlmMessage compressed;
    compressed.role = "assistant";
    compressed.text = "[compressed] summary";
    history.push_back(compressed);

    // Orphaned tool message (no matching
    // tool_calls in any assistant message)
    LlmMessage orphan_tool;
    orphan_tool.role = "tool";
    orphan_tool.tool_name = "send_to_session";
    orphan_tool.tool_call_id = "call_QPf0lost";
    orphan_tool.tool_result = {{"status", "ok"}};
    history.push_back(orphan_tool);

    // Normal user message
    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = "Hello";
    history.push_back(user_msg);

    ASSERT_EQ(history.size(), 3u);

    SessionStore::SanitizeHistory(history);

    // Orphaned tool should be removed
    ASSERT_EQ(history.size(), 2u);
    EXPECT_EQ(history[0].role, "assistant");
    EXPECT_EQ(history[1].role, "user");
}

TEST_F(SessionStoreTest,
    SanitizeKeepsValidToolPairs) {
    std::vector<LlmMessage> history;

    // User message
    LlmMessage user_msg;
    user_msg.role = "user";
    user_msg.text = "List apps";
    history.push_back(user_msg);

    // Assistant with tool_calls
    LlmMessage assistant_msg;
    assistant_msg.role = "assistant";
    assistant_msg.text = "Let me check.";
    LlmToolCall tc;
    tc.id = "call_valid123";
    tc.name = "list_apps";
    tc.args = {{"count", 5}};
    assistant_msg.tool_calls.push_back(tc);
    history.push_back(assistant_msg);

    // Valid tool result
    LlmMessage tool_msg;
    tool_msg.role = "tool";
    tool_msg.tool_name = "list_apps";
    tool_msg.tool_call_id = "call_valid123";
    tool_msg.tool_result = {{"apps", {"app1"}}};
    history.push_back(tool_msg);

    // Final assistant response
    LlmMessage final_msg;
    final_msg.role = "assistant";
    final_msg.text = "Found 1 app.";
    history.push_back(final_msg);

    ASSERT_EQ(history.size(), 4u);

    SessionStore::SanitizeHistory(history);

    // All messages should be preserved
    ASSERT_EQ(history.size(), 4u);
    EXPECT_EQ(history[0].role, "user");
    EXPECT_EQ(history[1].role, "assistant");
    EXPECT_EQ(history[2].role, "tool");
    EXPECT_EQ(history[3].role, "assistant");
}
