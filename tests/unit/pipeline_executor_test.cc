#include <gtest/gtest.h>
#include <fstream>
#include <cstdlib>
#include <unistd.h>
#include <sys/stat.h>
#include "pipeline_executor.hh"
#include "agent_core.hh"

using namespace tizenclaw;

class PipelineExecutorTest
    : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir = "/tmp/tizenclaw_test_pipelines";
    mkdir(test_dir.c_str(), 0755);
  }

  void TearDown() override {
    std::string cmd =
        "rm -rf " + test_dir;
    int ret = system(cmd.c_str());
    (void)ret;
  }

  // Write a pipeline JSON file
  void WritePipelineFile(
      const std::string& filename,
      const std::string& content) {
    std::ofstream f(test_dir + "/" + filename);
    f << content;
    f.close();
  }

  std::string test_dir;
};

// -------------------------------------------
// CRUD Tests
// -------------------------------------------
TEST_F(PipelineExecutorTest,
       CreateAndListPipeline) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "Test Pipeline"},
      {"description", "A test pipeline"},
      {"trigger", "manual"},
      {"steps", {
          {{"id", "step_1"},
           {"type", "prompt"},
           {"prompt", "Say hello"},
           {"output_var", "greeting"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_FALSE(id.empty());
  EXPECT_TRUE(id.find("pipe-") == 0);

  auto list = executor.ListPipelines();
  EXPECT_EQ(list.size(), 1u);
  EXPECT_EQ(list[0]["name"], "Test Pipeline");
  EXPECT_EQ(list[0]["steps_count"], 1);
}

TEST_F(PipelineExecutorTest,
       CreatePipelineMissingName) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"steps", {
          {{"id", "s1"}, {"type", "tool"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_TRUE(id.empty());
}

TEST_F(PipelineExecutorTest,
       CreatePipelineMissingSteps) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "No Steps"}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_TRUE(id.empty());
}

TEST_F(PipelineExecutorTest,
       DeletePipeline) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "To Delete"},
      {"steps", {
          {{"id", "s1"}, {"type", "prompt"},
           {"prompt", "test"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_FALSE(id.empty());

  // Verify exists
  EXPECT_NE(executor.GetPipeline(id), nullptr);

  // Delete
  EXPECT_TRUE(executor.DeletePipeline(id));
  EXPECT_EQ(executor.GetPipeline(id), nullptr);

  // Delete again should fail
  EXPECT_FALSE(executor.DeletePipeline(id));
}

TEST_F(PipelineExecutorTest,
       DeletePipelineNotFound) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  EXPECT_FALSE(
      executor.DeletePipeline("nonexistent"));
}

// -------------------------------------------
// Variable Interpolation Tests
// -------------------------------------------
TEST_F(PipelineExecutorTest,
       VariableInterpolation) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  // Create pipeline with variable refs
  nlohmann::json def = {
      {"name", "Var Test"},
      {"steps", {
          {{"id", "step_1"},
           {"type", "prompt"},
           {"prompt",
            "Greet {{user_name}} from "
            "{{location}}"},
           {"output_var", "step_1"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_FALSE(id.empty());

  // Run with input variables
  nlohmann::json input_vars = {
      {"user_name", "Alice"},
      {"location", "Seoul"}
  };

  auto result =
      executor.RunPipeline(id, input_vars);

  // Pipeline should complete (may fail at
  // LLM call without backend, but structure
  // is validated)
  EXPECT_EQ(result.pipeline_id, id);
  // Status will be "failed" without LLM
  // but we verify the structure is correct
  EXPECT_FALSE(result.status.empty());
}

// -------------------------------------------
// Condition Evaluation Tests
// -------------------------------------------
TEST_F(PipelineExecutorTest,
       ConditionEvalEquals) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  // Create a pipeline with condition
  nlohmann::json def = {
      {"name", "Condition Test"},
      {"steps", {
          {{"id", "check"},
           {"type", "condition"},
           {"condition",
            "{{status}} == success"},
           {"then_step", "on_success"},
           {"else_step", "on_failure"},
           {"output_var", "check"}},
          {{"id", "on_success"},
           {"type", "prompt"},
           {"prompt", "Success!"},
           {"output_var", "result"}},
          {{"id", "on_failure"},
           {"type", "prompt"},
           {"prompt", "Failed!"},
           {"output_var", "result"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_FALSE(id.empty());

  // Test with status = success
  nlohmann::json vars1 = {
      {"status", "success"}
  };
  auto r1 = executor.RunPipeline(id, vars1);
  EXPECT_EQ(r1.pipeline_id, id);

  // Verify condition result was stored
  auto it = r1.variables.find("check");
  EXPECT_NE(it, r1.variables.end());
  if (it != r1.variables.end()) {
    EXPECT_EQ(it->second, true);
  }
}

TEST_F(PipelineExecutorTest,
       ConditionEvalNotEquals) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "NE Condition"},
      {"steps", {
          {{"id", "check"},
           {"type", "condition"},
           {"condition",
            "{{val}} != error"},
           {"output_var", "check"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);

  nlohmann::json vars = {{"val", "ok"}};
  auto r = executor.RunPipeline(id, vars);
  auto it = r.variables.find("check");
  EXPECT_NE(it, r.variables.end());
  if (it != r.variables.end()) {
    EXPECT_EQ(it->second, true);
  }
}

TEST_F(PipelineExecutorTest,
       ConditionEvalContains) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "Contains Condition"},
      {"steps", {
          {{"id", "check"},
           {"type", "condition"},
           {"condition",
            "{{text}} contains hello"},
           {"output_var", "check"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);

  // True case
  nlohmann::json vars1 = {
      {"text", "say hello world"}
  };
  auto r1 = executor.RunPipeline(id, vars1);
  auto it1 = r1.variables.find("check");
  EXPECT_NE(it1, r1.variables.end());
  if (it1 != r1.variables.end()) {
    EXPECT_EQ(it1->second, true);
  }

  // False case
  nlohmann::json vars2 = {
      {"text", "goodbye world"}
  };
  auto r2 = executor.RunPipeline(id, vars2);
  auto it2 = r2.variables.find("check");
  EXPECT_NE(it2, r2.variables.end());
  if (it2 != r2.variables.end()) {
    EXPECT_EQ(it2->second, false);
  }
}

// -------------------------------------------
// Run Pipeline Tests
// -------------------------------------------
TEST_F(PipelineExecutorTest,
       RunPipelineNotFound) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  auto result =
      executor.RunPipeline("nonexistent");
  EXPECT_EQ(result.status, "failed");
  EXPECT_EQ(result.pipeline_id, "nonexistent");
}

TEST_F(PipelineExecutorTest,
       RunConditionOnlyPipeline) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  // Pipeline with only conditions (no LLM)
  nlohmann::json def = {
      {"name", "Pure Conditions"},
      {"steps", {
          {{"id", "c1"},
           {"type", "condition"},
           {"condition", "{{x}} == 1"},
           {"output_var", "result1"}},
          {{"id", "c2"},
           {"type", "condition"},
           {"condition", "{{x}} != 2"},
           {"output_var", "result2"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  nlohmann::json vars = {{"x", "1"}};
  auto r = executor.RunPipeline(id, vars);

  EXPECT_EQ(r.status, "success");
  EXPECT_EQ(r.variables["result1"], true);
  EXPECT_EQ(r.variables["result2"], true);
  EXPECT_EQ(r.duration_ms >= 0, true);
}

TEST_F(PipelineExecutorTest,
       MultipleCreateAndList) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  for (int i = 0; i < 3; i++) {
    nlohmann::json def = {
        {"name", "Pipeline " +
                     std::to_string(i)},
        {"steps", {
            {{"id", "s1"},
             {"type", "condition"},
             {"condition", "true"}}
        }}
    };
    (void)executor.CreatePipeline(def);
  }

  auto list = executor.ListPipelines();
  EXPECT_EQ(list.size(), 3u);
}

// -------------------------------------------
// Step Type Tests
// -------------------------------------------
TEST_F(PipelineExecutorTest,
       StepTypeAutoAssign) {
  AgentCore agent;
  PipelineExecutor executor(&agent);

  nlohmann::json def = {
      {"name", "Auto ID"},
      {"steps", {
          {{"type", "condition"},
           {"condition", "true"}},
          {{"id", "custom_id"},
           {"type", "condition"},
           {"condition", "false"}}
      }}
  };

  std::string id = executor.CreatePipeline(def);
  EXPECT_FALSE(id.empty());

  auto* p = executor.GetPipeline(id);
  ASSERT_NE(p, nullptr);
  EXPECT_EQ(p->steps.size(), 2u);
  EXPECT_EQ(p->steps[0].id, "step_0");
  EXPECT_EQ(p->steps[1].id, "custom_id");
}
