#include <gtest/gtest.h>

#include <cstdlib>
#include <fstream>
#include <memory>
#include <unistd.h>

#include "agent_core.hh"

using namespace tizenclaw;

class AgentCoreTest : public ::testing::Test {
 protected:
  void SetUp() override {
    // Create a dummy config for testing
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    config_path_ =
        std::string("test_llm_config_") +
        test_name + ".json";

    std::ofstream f(config_path_);
    f << "{\"active_backend\":\"ollama\","
         "\"backends\":{\"ollama\":{"
         "\"endpoint\":\"http://localhost:9999\","
         "\"model\":\"dummy\"}}}"
      << std::endl;
    f.close();
    setenv("TIZENCLAW_CONFIG_PATH",
           config_path_.c_str(), 1);

    agent_ = std::make_unique<AgentCore>();
  }

  void TearDown() override {
    agent_.reset();
    unlink(config_path_.c_str());
  }

  std::unique_ptr<AgentCore> agent_;
  std::string config_path_;
};

TEST_F(AgentCoreTest, InitializationTest) {
  // First initialization should succeed
  EXPECT_TRUE(agent_->Initialize());
  // Second initialization should safely return
  EXPECT_TRUE(agent_->Initialize());
}

TEST_F(AgentCoreTest, ProcessPromptWithoutInit) {
  // Without initialization, should return error
  std::string result =
      agent_->ProcessPrompt("test_session",
                            "Hello TizenClaw!");
  EXPECT_FALSE(result.empty());
  EXPECT_NE(result.find("Error"),
            std::string::npos);
}

TEST_F(AgentCoreTest, ProcessPromptReturnsString) {
  (void)agent_->Initialize();
  // In test without real LLM, may return error
  // but should return non-empty string
  std::string result =
      agent_->ProcessPrompt(
          "test_session",
          "What is the battery level?");
  EXPECT_FALSE(result.empty());
}

TEST_F(AgentCoreTest, IterativeLoopDetection) {
  (void)agent_->Initialize();
  std::string result =
      agent_->ProcessPrompt(
          "multi_step_session",
          "List apps and then check Wi-Fi.");
  EXPECT_FALSE(result.empty());
}

TEST_F(AgentCoreTest, ClearSessionTest) {
  (void)agent_->Initialize();
  // Process a prompt to create a session
  (void)agent_->ProcessPrompt(
      "clear_test_session", "Hello");
  // Clear should not crash
  agent_->ClearSession("clear_test_session");
  // Process again on same session should work
  std::string result =
      agent_->ProcessPrompt(
          "clear_test_session", "Hello again");
  EXPECT_FALSE(result.empty());
}

TEST_F(AgentCoreTest, ShutdownIdempotent) {
  (void)agent_->Initialize();
  // Shutdown should be safe to call multiple times
  agent_->Shutdown();
  agent_->Shutdown();
}
