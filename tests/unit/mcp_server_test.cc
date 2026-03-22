#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include "mcp_server.hh"
#include "agent_core.hh"

using namespace tizenclaw;


class McpServerTest : public ::testing::Test {
protected:
    void SetUp() override {
        // Dummy config for AgentCore
        const char* test_config =
            "test_mcp_llm_config.json";
        std::ofstream f(test_config);
        f << "{\"active_backend\":\"ollama\","
          << "\"backends\":{\"ollama\":{"
          << "\"endpoint\":\"http://localhost:9999\","
          << "\"model\":\"dummy\"}}}"
          << std::endl;
        f.close();
        setenv("TIZENCLAW_CONFIG_PATH",
               test_config, 1);

        agent = new AgentCore();
        (void)agent->Initialize();
        server = new McpServer(agent);
    }

    void TearDown() override {
        delete server;
        delete agent;
        unlink("test_mcp_llm_config.json");
    }

    AgentCore* agent;
    McpServer* server;
};

TEST_F(McpServerTest, InitializeResponse) {
    nlohmann::json req = {
        {"jsonrpc", "2.0"},
        {"id", 1},
        {"method", "initialize"},
        {"params", nlohmann::json::object()}
    };

    auto resp = server->ProcessRequest(req);

    EXPECT_EQ(resp["jsonrpc"], "2.0");
    EXPECT_EQ(resp["id"], 1);
    EXPECT_TRUE(resp.contains("result"));

    auto result = resp["result"];
    EXPECT_EQ(result["protocolVersion"],
              "2024-11-05");
    EXPECT_TRUE(result.contains("serverInfo"));
    EXPECT_EQ(result["serverInfo"]["name"],
              "TizenClaw-MCP-Server");
}

TEST_F(McpServerTest, ToolsListResponse) {
    nlohmann::json req = {
        {"jsonrpc", "2.0"},
        {"id", 2},
        {"method", "tools/list"},
        {"params", nlohmann::json::object()}
    };

    auto resp = server->ProcessRequest(req);
    auto result = resp["result"];

    EXPECT_TRUE(result.contains("tools"));
    EXPECT_TRUE(result["tools"].is_array());

    // At minimum, should have ask_tizenclaw
    bool found_ask = false;
    for (auto& t : result["tools"]) {
        if (t["name"] == "ask_tizenclaw") {
            found_ask = true;
            EXPECT_FALSE(
                t["description"].get<std::string>()
                    .empty());
            EXPECT_TRUE(t.contains("inputSchema"));
        }
    }
    EXPECT_TRUE(found_ask);
}

TEST_F(McpServerTest, MethodNotFound) {
    nlohmann::json req = {
        {"jsonrpc", "2.0"},
        {"id", 3},
        {"method", "unknown/method"},
        {"params", nlohmann::json::object()}
    };

    auto resp = server->ProcessRequest(req);

    EXPECT_TRUE(resp.contains("error"));
    EXPECT_EQ(resp["error"]["code"], -32601);
}

TEST_F(McpServerTest, NotificationNoResponse) {
    nlohmann::json req = {
        {"jsonrpc", "2.0"},
        {"method", "notifications/initialized"},
        {"params", nlohmann::json::object()}
    };

    auto resp = server->ProcessRequest(req);
    EXPECT_TRUE(resp.is_null());
}

TEST_F(McpServerTest, ToolCallNotFound) {
    nlohmann::json req = {
        {"jsonrpc", "2.0"},
        {"id", 4},
        {"method", "tools/call"},
        {"params", {
            {"name", "nonexistent_tool"},
            {"arguments", nlohmann::json::object()}
        }}
    };

    auto resp = server->ProcessRequest(req);
    auto result = resp["result"];

    EXPECT_TRUE(result["isError"].get<bool>());
}
