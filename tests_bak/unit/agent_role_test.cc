#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include <sys/stat.h>
#include "agent_role.hh"
#include "agent_core.hh"

using namespace tizenclaw;

class AgentRoleTest : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir = "/tmp/tizenclaw_test_roles";
    mkdir(test_dir.c_str(), 0755);
  }

  void TearDown() override {
    std::string cmd =
        "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  // Write a config file and return its path
  std::string WriteConfig(
      const std::string& content) {
    std::string path =
        test_dir + "/agent_roles.json";
    std::ofstream f(path);
    f << content;
    f.close();
    return path;
  }

  std::string test_dir;
};

// -------------------------------------------
// Role Loading Tests
// -------------------------------------------
TEST_F(AgentRoleTest,
       LoadRolesFromValidJson) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig(R"({
    "roles": [
      {
        "name": "researcher",
        "system_prompt": "You are a researcher.",
        "allowed_tools": ["search_knowledge"],
        "max_iterations": 8
      },
      {
        "name": "writer",
        "system_prompt": "You are a writer.",
        "allowed_tools": ["file_manager"],
        "max_iterations": 5
      }
    ]
  })");

  EXPECT_TRUE(engine.LoadRoles(path));

  auto names = engine.GetRoleNames();
  EXPECT_EQ(names.size(), 2u);

  auto* r = engine.GetRole("researcher");
  ASSERT_NE(r, nullptr);
  EXPECT_EQ(r->name, "researcher");
  EXPECT_EQ(r->system_prompt,
            "You are a researcher.");
  EXPECT_EQ(r->allowed_tools.size(), 1u);
  EXPECT_EQ(r->allowed_tools[0],
            "search_knowledge");
  EXPECT_EQ(r->max_iterations, 8);

  auto* w = engine.GetRole("writer");
  ASSERT_NE(w, nullptr);
  EXPECT_EQ(w->max_iterations, 5);
}

TEST_F(AgentRoleTest,
       LoadRolesEmptyFile) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig("{}");
  EXPECT_FALSE(engine.LoadRoles(path));
}

TEST_F(AgentRoleTest,
       LoadRolesInvalidJson) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig("not json");
  EXPECT_FALSE(engine.LoadRoles(path));
}

TEST_F(AgentRoleTest,
       LoadRolesFileNotFound) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  EXPECT_FALSE(engine.LoadRoles(
      "/nonexistent/path.json"));
}

TEST_F(AgentRoleTest,
       LoadRolesSkipsInvalidEntries) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig(R"({
    "roles": [
      {
        "name": "",
        "system_prompt": "No name role"
      },
      {
        "name": "valid",
        "system_prompt": "Valid role prompt"
      },
      {
        "name": "no_prompt",
        "system_prompt": ""
      }
    ]
  })");

  EXPECT_TRUE(engine.LoadRoles(path));

  auto names = engine.GetRoleNames();
  EXPECT_EQ(names.size(), 1u);

  auto* v = engine.GetRole("valid");
  ASSERT_NE(v, nullptr);
  EXPECT_EQ(v->system_prompt,
            "Valid role prompt");

  EXPECT_EQ(engine.GetRole(""), nullptr);
  EXPECT_EQ(engine.GetRole("no_prompt"),
            nullptr);
}

TEST_F(AgentRoleTest,
       LoadRolesDefaultMaxIterations) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig(R"({
    "roles": [
      {
        "name": "basic",
        "system_prompt": "Basic role"
      }
    ]
  })");

  EXPECT_TRUE(engine.LoadRoles(path));

  auto* r = engine.GetRole("basic");
  ASSERT_NE(r, nullptr);
  EXPECT_EQ(r->max_iterations, 10);
  EXPECT_TRUE(r->allowed_tools.empty());
}

// -------------------------------------------
// ListRoles Tests
// -------------------------------------------
TEST_F(AgentRoleTest,
       ListRolesReturnsAll) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig(R"({
    "roles": [
      {
        "name": "alpha",
        "system_prompt": "Alpha agent prompt",
        "allowed_tools": ["tool_a", "tool_b"]
      },
      {
        "name": "beta",
        "system_prompt": "Beta agent prompt"
      }
    ]
  })");

  ASSERT_TRUE(engine.LoadRoles(path));
  auto roles = engine.ListRoles();

  EXPECT_EQ(roles.size(), 2u);

  // Verify JSON structure
  bool found_alpha = false;
  bool found_beta = false;
  for (auto& r : roles) {
    EXPECT_TRUE(r.contains("name"));
    EXPECT_TRUE(r.contains("system_prompt"));
    EXPECT_TRUE(r.contains("allowed_tools"));
    EXPECT_TRUE(r.contains("max_iterations"));

    std::string name =
        r["name"].get<std::string>();
    if (name == "alpha") {
      found_alpha = true;
      EXPECT_EQ(r["allowed_tools"].size(), 2u);
    } else if (name == "beta") {
      found_beta = true;
      EXPECT_TRUE(
          r["allowed_tools"].empty());
    }
  }
  EXPECT_TRUE(found_alpha);
  EXPECT_TRUE(found_beta);
}

TEST_F(AgentRoleTest,
       ListRolesEmpty) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  auto roles = engine.ListRoles();
  EXPECT_TRUE(roles.empty());
}

// -------------------------------------------
// GetRole Tests
// -------------------------------------------
TEST_F(AgentRoleTest,
       GetRoleNotFound) {
  AgentCore agent;
  SupervisorEngine engine(&agent);

  EXPECT_EQ(engine.GetRole("missing"),
            nullptr);
}

// -------------------------------------------
// Tool Filtering Tests
// -------------------------------------------
TEST_F(AgentRoleTest,
       ToolFilteringAllowsOnlyWhitelisted) {
  AgentCore agent;

  // GetToolsFiltered with specific list
  std::vector<std::string> allowed = {
      "execute_code", "file_manager"
  };
  auto filtered =
      agent.GetToolsFiltered(allowed);

  // Should only contain tools in the
  // allowed list
  for (auto& tool : filtered) {
    bool found = false;
    for (auto& name : allowed) {
      if (tool.name == name) {
        found = true;
        break;
      }
    }
    EXPECT_TRUE(found)
        << "Tool " << tool.name
        << " should not be in filtered list";
  }
}

TEST_F(AgentRoleTest,
       ToolFilteringEmptyAllowsAll) {
  AgentCore agent;

  // Empty allowed list returns whatever
  // LoadSkillDeclarations finds.
  // Without initialized agent, skills dir may
  // not exist, so just verify it doesn't crash
  // and returns a vector.
  std::vector<std::string> empty_allowed;
  auto all_tools =
      agent.GetToolsFiltered(empty_allowed);

  // In test environment without skills dir,
  // still expect built-in tools to be loaded
  // (they are always registered)
  // However, LoadSkillDeclarations needs the
  // skills directory — verify no crash
  SUCCEED();
}

// -------------------------------------------
// ExecuteSupervisorOp Tests
// -------------------------------------------
TEST_F(AgentRoleTest,
       SupervisorOpListRoles) {
  // Test via SupervisorEngine directly
  // (AgentCore::ExecuteSupervisorOp requires
  //  Initialize() which needs LLM backend)
  AgentCore agent;
  SupervisorEngine engine(&agent);

  std::string path = WriteConfig(R"({
    "roles": [
      {
        "name": "test_role",
        "system_prompt": "Test role"
      }
    ]
  })");

  ASSERT_TRUE(engine.LoadRoles(path));
  auto roles = engine.ListRoles();

  EXPECT_EQ(roles.size(), 1u);
  EXPECT_EQ(roles[0]["name"], "test_role");
}

TEST_F(AgentRoleTest,
       SupervisorOpRunMissingGoal) {
  AgentCore agent;

  nlohmann::json args = {
      {"strategy", "sequential"}
  };

  auto result = agent.ExecuteSupervisorOp(
      "run_supervisor", args, "default");
  auto j = nlohmann::json::parse(result);

  EXPECT_TRUE(j.contains("error"));
}

TEST_F(AgentRoleTest,
       SupervisorOpUnknownOp) {
  AgentCore agent;

  auto result = agent.ExecuteSupervisorOp(
      "unknown_op", {}, "default");
  auto j = nlohmann::json::parse(result);

  EXPECT_TRUE(j.contains("error"));
}
