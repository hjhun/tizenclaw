#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include "audit_logger.hh"

using namespace tizenclaw;


class AuditLoggerTest
    : public ::testing::Test {
protected:
    void SetUp() override {
        const char* test_name = ::testing::UnitTest::GetInstance()->current_test_info()->name();
        test_dir_ = std::string("/tmp/tizenclaw_audit_test_") + test_name;
        int ret = system(("rm -rf " + test_dir_).c_str());
        (void)ret;
        AuditLogger::Instance().SetLogDir(
            test_dir_);
    }

    void TearDown() override {
        int ret = system(("rm -rf " + test_dir_).c_str());
        (void)ret;
    }

    std::string test_dir_;
};

TEST_F(AuditLoggerTest, TypeToStringWorks) {
    EXPECT_EQ(AuditLogger::TypeToString(
        AuditEventType::kIpcConnect),
        "ipc_connect");
    EXPECT_EQ(AuditLogger::TypeToString(
        AuditEventType::kToolExecution),
        "tool_execution");
    EXPECT_EQ(AuditLogger::TypeToString(
        AuditEventType::kToolBlocked),
        "tool_blocked");
}

TEST_F(AuditLoggerTest, MakeEventSetsFields) {
    auto event = AuditLogger::MakeEvent(
        AuditEventType::kToolExecution,
        "session1",
        {{"skill", "launch_app"}});

    EXPECT_EQ(event.type,
              AuditEventType::kToolExecution);
    EXPECT_EQ(event.session_id, "session1");
    EXPECT_FALSE(event.timestamp.empty());
    EXPECT_EQ(event.details["skill"],
              "launch_app");
}

TEST_F(AuditLoggerTest,
       LogCreatesMarkdownFile) {
    auto event = AuditLogger::MakeEvent(
        AuditEventType::kIpcAuth,
        "",
        {{"uid", 5001},
         {"allowed", true}});

    AuditLogger::Instance().Log(event);

    // Check file exists with today's date
    auto now = std::chrono::system_clock::now();
    auto t =
        std::chrono::system_clock::to_time_t(
            now);
    struct tm tm_buf;
    localtime_r(&t, &tm_buf);
    char date_buf[32];
    strftime(date_buf, sizeof(date_buf),
             "%Y-%m-%d", &tm_buf);

    std::string path =
        test_dir_ + "/" + date_buf + ".md";
    std::ifstream f(path);
    EXPECT_TRUE(f.is_open());

    // Read contents
    std::string content(
        (std::istreambuf_iterator<char>(f)),
        std::istreambuf_iterator<char>());
    f.close();

    // Verify YAML frontmatter
    EXPECT_NE(content.find("---"),
              std::string::npos);
    EXPECT_NE(content.find("type: audit_log"),
              std::string::npos);

    // Verify table header
    EXPECT_NE(
        content.find("| Time | Type | Session"),
        std::string::npos);

    // Verify event row
    EXPECT_NE(content.find("ipc_auth"),
              std::string::npos);
    EXPECT_NE(content.find("uid=5001"),
              std::string::npos);
}

TEST_F(AuditLoggerTest,
       MultipleEventsAppended) {
    AuditLogger::Instance().Log(
        AuditLogger::MakeEvent(
            AuditEventType::kIpcAuth,
            "", {{"uid", 100}}));
    AuditLogger::Instance().Log(
        AuditLogger::MakeEvent(
            AuditEventType::kToolExecution,
            "s1",
            {{"skill", "get_battery"}}));

    // Find today's file
    auto now = std::chrono::system_clock::now();
    auto t =
        std::chrono::system_clock::to_time_t(
            now);
    struct tm tm_buf;
    localtime_r(&t, &tm_buf);
    char date_buf[32];
    strftime(date_buf, sizeof(date_buf),
             "%Y-%m-%d", &tm_buf);

    std::string path =
        test_dir_ + "/" + date_buf + ".md";
    std::ifstream f(path);
    std::string content(
        (std::istreambuf_iterator<char>(f)),
        std::istreambuf_iterator<char>());
    f.close();

    // Should have both events
    EXPECT_NE(content.find("ipc_auth"),
              std::string::npos);
    EXPECT_NE(content.find("tool_execution"),
              std::string::npos);

    // Header should appear only once
    size_t first =
        content.find("## Audit Events");
    size_t second =
        content.find("## Audit Events",
                     first + 1);
    EXPECT_EQ(second, std::string::npos);
}

TEST_F(AuditLoggerTest, QueryReturnEvents) {
    AuditLogger::Instance().Log(
        AuditLogger::MakeEvent(
            AuditEventType::kToolExecution,
            "s1",
            {{"skill", "test_skill"}}));

    auto now = std::chrono::system_clock::now();
    auto t =
        std::chrono::system_clock::to_time_t(
            now);
    struct tm tm_buf;
    localtime_r(&t, &tm_buf);
    char date_buf[32];
    strftime(date_buf, sizeof(date_buf),
             "%Y-%m-%d", &tm_buf);

    auto results =
        AuditLogger::Instance().Query(date_buf);
    EXPECT_GE(results.size(), 1u);
}
