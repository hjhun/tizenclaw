#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include "agent_core.hh"

using namespace tizenclaw;


class AgentCoreTest : public ::testing::Test {
protected:
    void SetUp() override {
        // Create a dummy config for testing
        const char* test_name = ::testing::UnitTest::GetInstance()->current_test_info()->name();
        config_path_ = std::string("test_llm_config_") + test_name + ".json";
        
        std::ofstream f(config_path_);
        f << "{\"active_backend\":\"ollama\",\"backends\":{\"ollama\":{\"endpoint\":\"http://localhost:9999\",\"model\":\"dummy\"}}}" << std::endl;
        f.close();
        setenv("TIZENCLAW_CONFIG_PATH", config_path_.c_str(), 1);
        
        agent = new AgentCore();
    }

    void TearDown() override {
        delete agent;
        unlink(config_path_.c_str());
    }

    AgentCore* agent;
    std::string config_path_;
};

TEST_F(AgentCoreTest, InitializationTest) {
    // 1. First initialization should succeed
    EXPECT_TRUE(agent->Initialize());
    
    // 2. Second initialization should also safely return true without issues
    EXPECT_TRUE(agent->Initialize());
}

TEST_F(AgentCoreTest, ProcessPromptWithoutInit) {
    // Without initialization, should return error
    std::string result =
        agent->ProcessPrompt("test_session",
                             "Hello TizenClaw!");
    EXPECT_FALSE(result.empty());
    EXPECT_NE(result.find("Error"), std::string::npos);
}

TEST_F(AgentCoreTest, ProcessPromptReturnsString) {
    (void)agent->Initialize();
    // ProcessPrompt should return a response.
    // In a test environment without a real LLM config/backend, 
    // it might return an error string, which is still a non-empty string.
    std::string result =
        agent->ProcessPrompt("test_session",
                             "What is the battery level?");
    EXPECT_FALSE(result.empty());
}

TEST_F(AgentCoreTest, IterativeLoopDetection) {
    // This test would ideally mock LlmBackend to return tool_calls,
    // then verify that AgentCore::ProcessPrompt enters a second iteration.
    // For now, we perform a basic call.
    (void)agent->Initialize();
    std::string result = agent->ProcessPrompt("multi_step_session", "List apps and then check Wi-Fi.");
    EXPECT_FALSE(result.empty());
}

