#include <gtest/gtest.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <unistd.h>

#include "agent_core.hh"
#include "workflow_engine.hh"

using namespace tizenclaw;

namespace {

const std::string kBasicWorkflow = R"(---
name: System Check
description: Basic system health check
trigger: manual
---

## Step 1: Check Battery
- type: prompt
- instruction: Check the battery status
- output_var: battery_info

## Step 2: Check WiFi
- type: tool
- tool_name: get_wifi_info
- output_var: wifi_info

## Step 3: Report
- type: prompt
- instruction: Summarize {{battery_info}} and {{wifi_info}}
- output_var: report
)";

const std::string kMinimalWorkflow = R"(---
name: Minimal
---

## Step 1: Hello
- type: prompt
- instruction: Say hello
)";

const std::string kMultilineWorkflow = R"(---
name: Multiline Test
description: Test multiline instruction
trigger: manual
---

## Step 1: Analyze
- type: prompt
- instruction: |
    Please analyze the following data:
    Battery: {{battery_info}}
    WiFi: {{wifi_info}}
    Generate a comprehensive report.
- output_var: analysis
)";

}  // namespace

class WorkflowEngineTest : public ::testing::Test {
 protected:
  void SetUp() override {
    test_dir_ =
        "/tmp/tizenclaw_test_workflows";
    std::filesystem::create_directories(
        test_dir_);
  }

  void TearDown() override {
    std::filesystem::remove_all(test_dir_);
  }

  void WriteWorkflowFile(
      const std::string& filename,
      const std::string& content) {
    std::ofstream f(test_dir_ + "/" + filename);
    f << content;
    f.close();
  }

  std::string test_dir_;
};

// -------------------------------------------
// Markdown Parsing Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       ParseBasicMarkdown) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kBasicWorkflow);
  EXPECT_FALSE(id.empty());
  EXPECT_TRUE(id.find("wf-") == 0);

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_EQ(wf->name, "System Check");
  EXPECT_EQ(wf->description,
            "Basic system health check");
  EXPECT_EQ(wf->trigger, "manual");
  EXPECT_EQ(wf->steps.size(), 3u);

  // Step 1
  EXPECT_EQ(wf->steps[0].id, "step_1");
  EXPECT_EQ(wf->steps[0].description,
            "Check Battery");
  EXPECT_EQ(wf->steps[0].type,
            WorkflowStepType::kPrompt);
  EXPECT_EQ(wf->steps[0].instruction,
            "Check the battery status");
  EXPECT_EQ(wf->steps[0].output_var,
            "battery_info");

  // Step 2
  EXPECT_EQ(wf->steps[1].id, "step_2");
  EXPECT_EQ(wf->steps[1].type,
            WorkflowStepType::kTool);
  EXPECT_EQ(wf->steps[1].tool_name,
            "get_wifi_info");
  EXPECT_EQ(wf->steps[1].output_var,
            "wifi_info");

  // Step 3
  EXPECT_EQ(wf->steps[2].id, "step_3");
  EXPECT_EQ(wf->steps[2].type,
            WorkflowStepType::kPrompt);
  EXPECT_TRUE(wf->steps[2].instruction.find(
                  "{{battery_info}}") !=
              std::string::npos);
  EXPECT_EQ(wf->steps[2].output_var, "report");
}

TEST_F(WorkflowEngineTest,
       ParseMinimalMarkdown) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kMinimalWorkflow);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_EQ(wf->name, "Minimal");
  EXPECT_EQ(wf->trigger, "manual");
  EXPECT_EQ(wf->steps.size(), 1u);
  EXPECT_EQ(wf->steps[0].type,
            WorkflowStepType::kPrompt);
}

TEST_F(WorkflowEngineTest,
       ParseMultilineInstruction) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kMultilineWorkflow);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_EQ(wf->steps.size(), 1u);

  // Multiline instruction should be joined
  EXPECT_TRUE(wf->steps[0].instruction.find(
                  "analyze the following") !=
              std::string::npos);
  EXPECT_TRUE(wf->steps[0].instruction.find(
                  "{{battery_info}}") !=
              std::string::npos);
  EXPECT_EQ(wf->steps[0].output_var,
            "analysis");
}

// -------------------------------------------
// Error Handling Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       CreateWorkflowMissingName) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string md = R"(---
description: No name field
---

## Step 1: Test
- type: prompt
- instruction: Hello
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_TRUE(id.empty());
}

TEST_F(WorkflowEngineTest,
       CreateWorkflowMissingSteps) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string md = R"(---
name: No Steps
---
This workflow has no step sections.
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_TRUE(id.empty());
}

TEST_F(WorkflowEngineTest,
       CreateWorkflowEmptyMarkdown) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id = engine.CreateWorkflow("");
  EXPECT_TRUE(id.empty());
}

// -------------------------------------------
// CRUD Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       CreateAndListWorkflow) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kBasicWorkflow);
  EXPECT_FALSE(id.empty());

  auto list = engine.ListWorkflows();
  EXPECT_EQ(list.size(), 1u);
  EXPECT_EQ(list[0]["name"], "System Check");
  EXPECT_EQ(list[0]["steps_count"], 3);
}

TEST_F(WorkflowEngineTest,
       DeleteWorkflow) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kBasicWorkflow);
  EXPECT_FALSE(id.empty());

  EXPECT_NE(engine.GetWorkflow(id), nullptr);
  EXPECT_TRUE(engine.DeleteWorkflow(id));
  EXPECT_EQ(engine.GetWorkflow(id), nullptr);

  // Delete nonexistent
  EXPECT_FALSE(engine.DeleteWorkflow(id));
}

TEST_F(WorkflowEngineTest,
       DeleteWorkflowNotFound) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  EXPECT_FALSE(
      engine.DeleteWorkflow("nonexistent"));
}

TEST_F(WorkflowEngineTest,
       MultipleCreateAndList) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  engine.CreateWorkflow(kBasicWorkflow);
  engine.CreateWorkflow(kMinimalWorkflow);

  auto list = engine.ListWorkflows();
  EXPECT_EQ(list.size(), 2u);
}

// -------------------------------------------
// Run Workflow Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       RunWorkflowNotFound) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  auto result =
      engine.RunWorkflow("nonexistent");
  EXPECT_EQ(result.status, "failed");
  EXPECT_EQ(result.workflow_id, "nonexistent");
}

TEST_F(WorkflowEngineTest,
       RunWorkflowStructure) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kBasicWorkflow);
  EXPECT_FALSE(id.empty());

  // Run will fail without LLM backend,
  // but we verify the structure
  auto result = engine.RunWorkflow(id);
  EXPECT_EQ(result.workflow_id, id);
  EXPECT_FALSE(result.status.empty());
  EXPECT_GE(result.duration_ms, 0);
}

TEST_F(WorkflowEngineTest,
       RunWorkflowWithInputVars) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string id =
      engine.CreateWorkflow(kBasicWorkflow);
  EXPECT_FALSE(id.empty());

  nlohmann::json input_vars = {
      {"device_name", "TizenTV"},
      {"user", "tester"}};

  auto result =
      engine.RunWorkflow(id, input_vars);
  EXPECT_EQ(result.workflow_id, id);
  EXPECT_FALSE(result.status.empty());
}

// -------------------------------------------
// Step Type Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       StepTypeDefaults) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  // Default step type should be prompt
  std::string md = R"(---
name: Default Type
---

## Step 1: NoType
- instruction: Just do something
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_EQ(wf->steps[0].type,
            WorkflowStepType::kPrompt);
}

TEST_F(WorkflowEngineTest,
       ToolStepType) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string md = R"(---
name: Tool Type
---

## Step 1: RunTool
- type: tool
- tool_name: get_device_info
- output_var: info
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_EQ(wf->steps[0].type,
            WorkflowStepType::kTool);
  EXPECT_EQ(wf->steps[0].tool_name,
            "get_device_info");
  EXPECT_EQ(wf->steps[0].output_var, "info");
}

// -------------------------------------------
// Skip on Failure / Retry Tests
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       StepSkipOnFailure) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string md = R"(---
name: Skip Test
---

## Step 1: MayFail
- type: prompt
- instruction: Do something risky
- skip_on_failure: true
- max_retries: 2
- output_var: risky_result
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  EXPECT_TRUE(wf->steps[0].skip_on_failure);
  EXPECT_EQ(wf->steps[0].max_retries, 2);
}

// -------------------------------------------
// Output Variable Auto-Assignment
// -------------------------------------------
TEST_F(WorkflowEngineTest,
       AutoAssignOutputVar) {
  AgentCore agent;
  WorkflowEngine engine(&agent);

  std::string md = R"(---
name: Auto Var
---

## Step 1: NoOutputVar
- type: prompt
- instruction: Do something
)";

  std::string id = engine.CreateWorkflow(md);
  EXPECT_FALSE(id.empty());

  const auto* wf = engine.GetWorkflow(id);
  ASSERT_NE(wf, nullptr);
  // Should auto-assign step ID as output_var
  EXPECT_EQ(wf->steps[0].output_var, "step_1");
}
