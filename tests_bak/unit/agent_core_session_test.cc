#include <gtest/gtest.h>
#include "agent_core.hh"

using namespace tizenclaw;

class AgentCoreSessionTest : public ::testing::Test {
protected:
    void SetUp() override {
        // Create a dummy config for testing
        // (agent won't be fully initialized,
        // but session operations don't require LLM)
    }
};

TEST_F(AgentCoreSessionTest, CreateSession) {
    AgentCore agent;
    nlohmann::json args = {
        {"name", "researcher"},
        {"system_prompt",
         "You are a research assistant."}
    };

    auto result = agent.ExecuteSessionOp(
        "create_session", args, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_EQ(j["status"], "ok");
    EXPECT_TRUE(
        j["session_id"].get<std::string>()
            .find("agent_researcher_") !=
        std::string::npos);
    EXPECT_EQ(j["name"], "researcher");
    EXPECT_GT(
        j["system_prompt_length"].get<int>(), 0);
}

TEST_F(AgentCoreSessionTest,
    CreateSessionNoPrompt) {
    AgentCore agent;
    nlohmann::json args = {
        {"name", "test"},
        {"system_prompt", ""}
    };

    auto result = agent.ExecuteSessionOp(
        "create_session", args, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_TRUE(j.contains("error"));
}

TEST_F(AgentCoreSessionTest, ListSessions) {
    AgentCore agent;

    // Create two sessions
    nlohmann::json args1 = {
        {"name", "alpha"},
        {"system_prompt", "Alpha agent prompt"}
    };
    nlohmann::json args2 = {
        {"name", "beta"},
        {"system_prompt", "Beta agent prompt"}
    };
    agent.ExecuteSessionOp(
        "create_session", args1, "default");
    agent.ExecuteSessionOp(
        "create_session", args2, "default");

    // List sessions
    auto result = agent.ExecuteSessionOp(
        "list_sessions", {}, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_EQ(j["status"], "ok");
    EXPECT_GE(j["count"].get<int>(), 2);
}

TEST_F(AgentCoreSessionTest,
    SendToSessionSelfBlocked) {
    AgentCore agent;

    nlohmann::json args = {
        {"target_session", "default"},
        {"message", "hello"}
    };

    auto result = agent.ExecuteSessionOp(
        "send_to_session", args, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_TRUE(j.contains("error"));
    EXPECT_TRUE(
        j["error"].get<std::string>()
            .find("self") != std::string::npos);
}

TEST_F(AgentCoreSessionTest,
    SendToSessionMissingArgs) {
    AgentCore agent;

    // Missing message
    nlohmann::json args = {
        {"target_session", "some_session"}
    };

    auto result = agent.ExecuteSessionOp(
        "send_to_session", args, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_TRUE(j.contains("error"));
}

TEST_F(AgentCoreSessionTest,
    UnknownOperation) {
    AgentCore agent;

    auto result = agent.ExecuteSessionOp(
        "delete_session", {}, "default");
    auto j = nlohmann::json::parse(result);

    EXPECT_TRUE(j.contains("error"));
}
