#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <sys/stat.h>
#include "a2a_handler.hh"
#include "agent_core.hh"

using namespace tizenclaw;

class A2AHandlerTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir = "/tmp/tizenclaw_test_a2a";
    mkdir(test_dir.c_str(), 0755);
  }

  void TearDown() override {
    std::string cmd =
        "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  std::string test_dir;
};

// -------------------------------------------
// Agent Card Tests
// -------------------------------------------
TEST_F(A2AHandlerTest, AgentCardDefault) {
  AgentCore agent;
  A2AHandler handler(&agent);

  auto card = handler.GetAgentCard();

  EXPECT_EQ(card["name"],
            "TizenClaw Agent");
  EXPECT_EQ(card["protocol"], "a2a");
  EXPECT_EQ(card["protocolVersion"], "0.1");
  EXPECT_TRUE(
      card.contains("capabilities"));
  EXPECT_TRUE(
      card.contains("authentication"));
  EXPECT_TRUE(
      card.contains("skills"));

  auto skills = card["skills"];
  EXPECT_GE(skills.size(), 1u);
}

TEST_F(A2AHandlerTest,
       AgentCardWithConfig) {
  AgentCore agent;
  A2AHandler handler(&agent);

  // Write config
  std::string config_path =
      test_dir + "/a2a_config.json";
  std::ofstream f(config_path);
  f << R"({
    "agent_name": "Custom Agent",
    "agent_description": "Custom Desc",
    "agent_url": "https://example.com",
    "bearer_tokens": ["token1", "token2"]
  })";
  f.close();

  EXPECT_TRUE(
      handler.LoadConfig(config_path));

  auto card = handler.GetAgentCard();
  EXPECT_EQ(card["name"], "Custom Agent");
  EXPECT_EQ(card["description"],
            "Custom Desc");
  EXPECT_EQ(card["url"],
            "https://example.com");
}

// -------------------------------------------
// Bearer Token Tests
// -------------------------------------------
TEST_F(A2AHandlerTest,
       BearerTokenNoConfig) {
  AgentCore agent;
  A2AHandler handler(&agent);

  // No tokens configured = allow all
  EXPECT_TRUE(
      handler.ValidateBearerToken("anything"));
  EXPECT_TRUE(
      handler.ValidateBearerToken(""));
}

TEST_F(A2AHandlerTest,
       BearerTokenWithConfig) {
  AgentCore agent;
  A2AHandler handler(&agent);

  std::string config_path =
      test_dir + "/a2a_config.json";
  std::ofstream f(config_path);
  f << R"({
    "bearer_tokens": ["secret123", "token456"]
  })";
  f.close();

  handler.LoadConfig(config_path);

  EXPECT_TRUE(
      handler.ValidateBearerToken("secret123"));
  EXPECT_TRUE(
      handler.ValidateBearerToken("token456"));
  EXPECT_FALSE(
      handler.ValidateBearerToken("wrong"));
  EXPECT_FALSE(
      handler.ValidateBearerToken(""));
}

// -------------------------------------------
// JSON-RPC Tests
// -------------------------------------------
TEST_F(A2AHandlerTest,
       JsonRpcInvalidRequest) {
  AgentCore agent;
  A2AHandler handler(&agent);

  // Missing jsonrpc field
  nlohmann::json req = {
      {"method", "tasks/get"},
      {"id", 1}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_TRUE(resp.contains("error"));
  EXPECT_EQ(resp["error"]["code"], -32600);
}

TEST_F(A2AHandlerTest,
       JsonRpcMissingMethod) {
  AgentCore agent;
  A2AHandler handler(&agent);

  nlohmann::json req = {
      {"jsonrpc", "2.0"},
      {"id", 1}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_TRUE(resp.contains("error"));
  EXPECT_EQ(resp["error"]["code"], -32600);
}

TEST_F(A2AHandlerTest,
       JsonRpcMethodNotFound) {
  AgentCore agent;
  A2AHandler handler(&agent);

  nlohmann::json req = {
      {"jsonrpc", "2.0"},
      {"method", "unknown/method"},
      {"id", 1}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_TRUE(resp.contains("error"));
  EXPECT_EQ(resp["error"]["code"], -32601);
}

TEST_F(A2AHandlerTest,
       JsonRpcTaskGetNotFound) {
  AgentCore agent;
  A2AHandler handler(&agent);

  nlohmann::json req = {
      {"jsonrpc", "2.0"},
      {"method", "tasks/get"},
      {"id", 1},
      {"params", {{"id", "nonexistent"}}}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_EQ(resp["jsonrpc"], "2.0");
  EXPECT_EQ(resp["id"], 1);
  // Result contains error field
  EXPECT_TRUE(
      resp["result"].contains("error"));
}

TEST_F(A2AHandlerTest,
       JsonRpcTaskCancelNotFound) {
  AgentCore agent;
  A2AHandler handler(&agent);

  nlohmann::json req = {
      {"jsonrpc", "2.0"},
      {"method", "tasks/cancel"},
      {"id", 2},
      {"params", {{"id", "nonexistent"}}}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_EQ(resp["jsonrpc"], "2.0");
  EXPECT_EQ(resp["id"], 2);
  EXPECT_TRUE(
      resp["result"].contains("error"));
}

TEST_F(A2AHandlerTest,
       JsonRpcTaskSendMissingMessage) {
  AgentCore agent;
  A2AHandler handler(&agent);

  nlohmann::json req = {
      {"jsonrpc", "2.0"},
      {"method", "tasks/send"},
      {"id", 3},
      {"params", nlohmann::json::object()}
  };

  auto resp = handler.HandleJsonRpc(req);
  EXPECT_EQ(resp["jsonrpc"], "2.0");
  EXPECT_TRUE(
      resp["result"].contains("error"));
}

// -------------------------------------------
// Config Loading Tests
// -------------------------------------------
TEST_F(A2AHandlerTest,
       LoadConfigMissingFile) {
  AgentCore agent;
  A2AHandler handler(&agent);

  EXPECT_FALSE(
      handler.LoadConfig(
          "/nonexistent/config.json"));
}

TEST_F(A2AHandlerTest,
       LoadConfigInvalidJson) {
  AgentCore agent;
  A2AHandler handler(&agent);

  std::string config_path =
      test_dir + "/bad_config.json";
  std::ofstream f(config_path);
  f << "not valid json{";
  f.close();

  EXPECT_FALSE(
      handler.LoadConfig(config_path));
}
