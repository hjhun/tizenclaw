#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <string>

#include "tool_indexer.hh"

using namespace tizenclaw;

namespace fs = std::filesystem;

class ToolIndexerTest : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    test_dir_ =
        std::string("test_tool_indexer_") +
        test_name;
    fs::create_directories(test_dir_);
    fs::create_directories(
        test_dir_ + "/skills");
    fs::create_directories(
        test_dir_ + "/custom_skills");
    fs::create_directories(
        test_dir_ + "/actions");
    fs::create_directories(
        test_dir_ + "/embedded");
  }

  void TearDown() override {
    std::error_code ec;
    fs::remove_all(test_dir_, ec);
  }

  void CreateSkillManifest(
      const std::string& subdir,
      const std::string& skill_name,
      const std::string& desc,
      const std::string& category = "",
      const std::string& risk = "low") {
    std::string dir =
        test_dir_ + "/" + subdir + "/" +
        skill_name;
    fs::create_directories(dir);
    std::ofstream f(dir + "/manifest.json");
    f << R"({"name":")" << skill_name << R"(",)";
    if (!category.empty())
      f << R"("category":")" << category
        << R"(",)";
    f << R"("description":")" << desc
      << R"(","risk_level":")" << risk
      << R"(","parameters":{"type":"object"}})"
      << std::endl;
  }

  void CreateToolMd(const std::string& subdir,
                    const std::string& name,
                    const std::string& title,
                    const std::string& body,
                    const std::string& cat = "") {
    std::string path =
        test_dir_ + "/" + subdir + "/" +
        name + ".md";
    std::ofstream f(path);
    f << "# " << title << "\n\n";
    if (!cat.empty())
      f << "**Category**: " << cat << "\n\n";
    f << body << "\n";
  }

  std::string ReadFile(const std::string& path) {
    std::ifstream in(path);
    if (!in.is_open()) return "";
    return {std::istreambuf_iterator<char>(in),
            std::istreambuf_iterator<char>()};
  }

  std::string test_dir_;
};

TEST_F(ToolIndexerTest,
       SkillsGroupedByCategory) {
  CreateSkillManifest("skills",
                      "get_battery_info",
                      "Get battery level",
                      "Device Info");
  CreateSkillManifest("skills",
                      "list_apps",
                      "List installed apps",
                      "App Management");
  CreateSkillManifest("skills",
                      "get_wifi_info",
                      "Get WiFi info",
                      "Network");

  ToolIndexer::GenerateSkillsIndex(
      test_dir_ + "/skills");

  std::string index =
      ReadFile(test_dir_ + "/skills/index.md");
  EXPECT_FALSE(index.empty());

  // Verify category headers exist
  EXPECT_NE(index.find("### App Management"),
            std::string::npos);
  EXPECT_NE(index.find("### Device Info"),
            std::string::npos);
  EXPECT_NE(index.find("### Network"),
            std::string::npos);

  // Verify tools are listed
  EXPECT_NE(index.find("get_battery_info"),
            std::string::npos);
  EXPECT_NE(index.find("list_apps"),
            std::string::npos);
  EXPECT_NE(index.find("Total: 3"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       UncategorizedSkillsFallback) {
  CreateSkillManifest("skills",
                      "no_category_skill",
                      "A skill with no category");

  ToolIndexer::GenerateSkillsIndex(
      test_dir_ + "/skills");

  std::string index =
      ReadFile(test_dir_ + "/skills/index.md");
  EXPECT_NE(index.find("### Uncategorized"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       EmbeddedGroupedByCategory) {
  CreateToolMd("embedded", "execute_code",
               "execute_code",
               "Execute Python code.",
               "code_execution");
  CreateToolMd("embedded", "create_task",
               "create_task",
               "Create scheduled task.",
               "task_scheduler");

  ToolIndexer::GenerateEmbeddedIndex(
      test_dir_ + "/embedded");

  std::string index =
      ReadFile(test_dir_ + "/embedded/index.md");
  EXPECT_FALSE(index.empty());

  EXPECT_NE(index.find("### code_execution"),
            std::string::npos);
  EXPECT_NE(index.find("### task_scheduler"),
            std::string::npos);
  EXPECT_NE(index.find("Total: 2"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       ActionsWithoutCategory) {
  CreateToolMd("actions", "power_off",
               "power_off",
               "Turn off the device.");

  ToolIndexer::GenerateActionsIndex(
      test_dir_ + "/actions");

  std::string index =
      ReadFile(test_dir_ + "/actions/index.md");
  EXPECT_NE(index.find("### Uncategorized"),
            std::string::npos);
  EXPECT_NE(index.find("power_off"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       GenerateToolsMd) {
  CreateSkillManifest("skills",
                      "get_info",
                      "Get device info",
                      "Device Info");
  CreateToolMd("embedded", "file_manager",
               "file_manager",
               "Manage files on device.",
               "file_system");

  ToolIndexer::RegenerateAll(test_dir_);

  std::string tools_md =
      ReadFile(test_dir_ + "/tools.md");
  EXPECT_FALSE(tools_md.empty());
  EXPECT_NE(tools_md.find("## Skills"),
            std::string::npos);
  EXPECT_NE(tools_md.find("## Embedded Tools"),
            std::string::npos);
  EXPECT_NE(tools_md.find("get_info"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       RegenerateAllCreatesAllFiles) {
  CreateSkillManifest("skills",
                      "test_skill",
                      "A test skill",
                      "Testing");
  CreateSkillManifest("custom_skills",
                      "my_custom",
                      "Custom skill",
                      "Utility");
  CreateToolMd("actions", "act1",
               "act1", "Action 1.");
  CreateToolMd("embedded", "emb1",
               "emb1", "Embedded 1.",
               "code_execution");

  ToolIndexer::RegenerateAll(test_dir_);

  EXPECT_TRUE(fs::exists(
      test_dir_ + "/skills/index.md"));
  EXPECT_TRUE(fs::exists(
      test_dir_ + "/custom_skills/index.md"));
  EXPECT_TRUE(fs::exists(
      test_dir_ + "/actions/index.md"));
  EXPECT_TRUE(fs::exists(
      test_dir_ + "/embedded/index.md"));
  EXPECT_TRUE(fs::exists(
      test_dir_ + "/tools.md"));
}

TEST_F(ToolIndexerTest,
       EmptyDirectoriesDoNotCrash) {
  ToolIndexer::RegenerateAll(test_dir_);

  EXPECT_TRUE(fs::exists(
      test_dir_ + "/tools.md"));
}

// ─── manifest.json v2 schema tests ───

TEST_F(ToolIndexerTest,
       V1ManifestBackwardCompat) {
  // v1 manifest WITHOUT execution/output
  // sections must still parse correctly
  CreateSkillManifest("skills",
                      "v1_legacy_skill",
                      "A legacy v1 skill",
                      "Device Info", "low");

  ToolIndexer::GenerateSkillsIndex(
      test_dir_ + "/skills");

  std::string index =
      ReadFile(test_dir_ + "/skills/index.md");
  EXPECT_NE(index.find("v1_legacy_skill"),
            std::string::npos);
  EXPECT_NE(index.find("A legacy v1 skill"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       V2ManifestWithExecutionAndOutput) {
  // v2 manifest WITH execution.mode and
  // output.streaming — should parse without
  // error and include the skill in the index
  std::string dir =
      test_dir_ + "/skills/streaming_skill";
  fs::create_directories(dir);
  std::ofstream f(dir + "/manifest.json");
  f << R"({
    "name": "streaming_skill",
    "category": "System Actions",
    "description": "A skill that streams output",
    "risk_level": "medium",
    "execution": {
      "mode": "streaming",
      "timeout_ms": 30000,
      "entrypoint": "python3 streaming_skill.py"
    },
    "output": {
      "streaming": true,
      "progress_events": ["download_progress"],
      "content_types": ["application/json"]
    },
    "parameters": {
      "type": "object",
      "properties": {
        "url": {
          "type": "string",
          "description": "URL to download"
        }
      },
      "required": ["url"]
    }
  })";
  f.close();

  ToolIndexer::GenerateSkillsIndex(
      test_dir_ + "/skills");

  std::string index =
      ReadFile(test_dir_ + "/skills/index.md");
  EXPECT_NE(index.find("streaming_skill"),
            std::string::npos);
  EXPECT_NE(index.find("System Actions"),
            std::string::npos);
  EXPECT_NE(index.find("medium"),
            std::string::npos);
}

TEST_F(ToolIndexerTest,
       V2ManifestMixedWithV1) {
  // Mix of v1 and v2 manifests in the same
  // directory — both should appear in the index
  CreateSkillManifest("skills",
                      "old_skill",
                      "A v1 skill",
                      "Network", "low");

  std::string dir =
      test_dir_ + "/skills/new_skill";
  fs::create_directories(dir);
  std::ofstream f(dir + "/manifest.json");
  f << R"({
    "name": "new_skill",
    "category": "Network",
    "description": "A v2 streaming skill",
    "risk_level": "low",
    "execution": {"mode": "streaming"},
    "output": {"streaming": true},
    "parameters": {"type": "object"}
  })";
  f.close();

  ToolIndexer::GenerateSkillsIndex(
      test_dir_ + "/skills");

  std::string index =
      ReadFile(test_dir_ + "/skills/index.md");
  EXPECT_NE(index.find("old_skill"),
            std::string::npos);
  EXPECT_NE(index.find("new_skill"),
            std::string::npos);
  EXPECT_NE(index.find("Total: 2"),
            std::string::npos);
}

