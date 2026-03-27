#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include <sys/stat.h>
#include "agent_factory.hh"
#include "agent_core.hh"

using namespace tizenclaw;

class AgentFactoryTest : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir = "/tmp/tizenclaw_test_factory";
    mkdir(test_dir.c_str(), 0755);

    agent = std::make_unique<AgentCore>();
    supervisor = std::make_unique<SupervisorEngine>(
        agent.get());
    factory = std::make_unique<AgentFactory>(
        agent.get(), supervisor.get());
  }

  void TearDown() override {
    factory.reset();
    supervisor.reset();
    agent.reset();
    std::string cmd = "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  std::string test_dir;
  std::unique_ptr<AgentCore> agent;
  std::unique_ptr<SupervisorEngine> supervisor;
  std::unique_ptr<AgentFactory> factory;
};

// -------------------------------------------
// SpawnAgent Tests
// -------------------------------------------
TEST_F(AgentFactoryTest,
       SpawnAgentSuccess) {
  nlohmann::json args = {
      {"name", "test_analyst"},
      {"system_prompt",
       "You are a test analyst agent."},
      {"max_iterations", 5}};

  auto result_str = factory->SpawnAgent(args);
  auto result = nlohmann::json::parse(result_str);

  EXPECT_EQ(result["status"], "ok");
  EXPECT_EQ(result["agent_name"], "test_analyst");
  EXPECT_FALSE(result["persistent"]);
}

TEST_F(AgentFactoryTest,
       SpawnAgentInvalidNameTooShort) {
  nlohmann::json args = {
      {"name", "ab"},
      {"system_prompt", "Test prompt"}};

  auto result_str = factory->SpawnAgent(args);
  auto result = nlohmann::json::parse(result_str);

  EXPECT_TRUE(result.contains("error"));
}

TEST_F(AgentFactoryTest,
       SpawnAgentInvalidNameUppercase) {
  nlohmann::json args = {
      {"name", "TestAgent"},
      {"system_prompt", "Test prompt"}};

  auto result_str = factory->SpawnAgent(args);
  auto result = nlohmann::json::parse(result_str);

  EXPECT_TRUE(result.contains("error"));
}

TEST_F(AgentFactoryTest,
       SpawnAgentEmptyPrompt) {
  nlohmann::json args = {
      {"name", "valid_name"},
      {"system_prompt", ""}};

  auto result_str = factory->SpawnAgent(args);
  auto result = nlohmann::json::parse(result_str);

  EXPECT_TRUE(result.contains("error"));
}

TEST_F(AgentFactoryTest,
       SpawnAgentDuplicateName) {
  nlohmann::json args = {
      {"name", "unique_agent"},
      {"system_prompt", "First agent"}};

  auto r1 = nlohmann::json::parse(
      factory->SpawnAgent(args));
  EXPECT_EQ(r1["status"], "ok");

  // Try to create again with same name
  auto r2 = nlohmann::json::parse(
      factory->SpawnAgent(args));
  EXPECT_TRUE(r2.contains("error"));
}

TEST_F(AgentFactoryTest,
       SpawnAgentMaxLimitReached) {
  // Create max dynamic agents
  for (size_t i = 0; i < 5; ++i) {
    nlohmann::json args = {
        {"name", "agent_" + std::string(1, 'a' + i)},
        {"system_prompt", "Agent " + std::to_string(i)}};
    auto r = nlohmann::json::parse(
        factory->SpawnAgent(args));
    EXPECT_EQ(r["status"], "ok");
  }

  // Try to create one more
  nlohmann::json args = {
      {"name", "one_too_many"},
      {"system_prompt", "Over limit"}};
  auto r = nlohmann::json::parse(
      factory->SpawnAgent(args));
  EXPECT_TRUE(r.contains("error"));
}

// -------------------------------------------
// ListDynamicAgents Tests
// -------------------------------------------
TEST_F(AgentFactoryTest,
       ListDynamicAgentsEmpty) {
  auto agents = factory->ListDynamicAgents();
  EXPECT_TRUE(agents.empty());
}

TEST_F(AgentFactoryTest,
       ListDynamicAgentsAfterSpawn) {
  nlohmann::json args = {
      {"name", "test_agent"},
      {"system_prompt", "A test agent"},
      {"allowed_tools",
       nlohmann::json::array(
           {"execute_code", "file_manager"})}};

  factory->SpawnAgent(args);

  auto agents = factory->ListDynamicAgents();
  EXPECT_EQ(agents.size(), 1u);
  EXPECT_EQ(agents[0]["name"], "test_agent");
  EXPECT_EQ(agents[0]["allowed_tools"].size(), 2u);
}

// -------------------------------------------
// RemoveAgent Tests
// -------------------------------------------
TEST_F(AgentFactoryTest,
       RemoveAgentSuccess) {
  nlohmann::json args = {
      {"name", "removable"},
      {"system_prompt", "To be removed"}};
  factory->SpawnAgent(args);

  auto r = nlohmann::json::parse(
      factory->RemoveAgent("removable"));
  EXPECT_EQ(r["status"], "ok");
  EXPECT_EQ(r["removed"], "removable");

  // Verify it's gone
  auto agents = factory->ListDynamicAgents();
  EXPECT_TRUE(agents.empty());
}

TEST_F(AgentFactoryTest,
       RemoveAgentNotFound) {
  auto r = nlohmann::json::parse(
      factory->RemoveAgent("nonexistent"));
  EXPECT_TRUE(r.contains("error"));
}

// -------------------------------------------
// SupervisorEngine Integration
// -------------------------------------------
TEST_F(AgentFactoryTest,
       SpawnRegistersWithSupervisor) {
  nlohmann::json args = {
      {"name", "registered_agent"},
      {"system_prompt", "Supervisor test"}};

  factory->SpawnAgent(args);

  // Verify role exists in supervisor
  auto* role = supervisor->GetRole(
      "registered_agent");
  ASSERT_NE(role, nullptr);
  EXPECT_EQ(role->system_prompt,
            "Supervisor test");
}

TEST_F(AgentFactoryTest,
       RemoveUnregistersFromSupervisor) {
  nlohmann::json args = {
      {"name", "temp_agent"},
      {"system_prompt", "Temporary"}};

  factory->SpawnAgent(args);
  EXPECT_NE(
      supervisor->GetRole("temp_agent"),
      nullptr);

  factory->RemoveAgent("temp_agent");
  EXPECT_EQ(
      supervisor->GetRole("temp_agent"),
      nullptr);
}
