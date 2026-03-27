#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>

#include <json.hpp>

#include "skill_manifest.hh"
#include "skill_verifier.hh"

using namespace tizenclaw;

class SkillManifestTest : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    namespace fs = std::filesystem;
    test_dir_ = fs::absolute(
        std::string("/tmp/test_skill_manifest_") +
        test_name).string();
    fs::create_directories(test_dir_);
  }

  void TearDown() override {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::remove_all(test_dir_, ec);
  }

  void WriteSkillMd(const std::string& content) {
    std::ofstream f(test_dir_ + "/SKILL.md");
    f << content;
    f.close();
  }

  void WriteManifestJson(const nlohmann::json& j) {
    std::ofstream f(test_dir_ + "/manifest.json");
    f << j.dump(4) << std::endl;
    f.close();
  }

  void WriteScript(const std::string& name,
                   const std::string& content) {
    std::ofstream f(test_dir_ + "/" + name);
    f << content;
    f.close();
  }

  std::string test_dir_;
};

// Test: SKILL.md is correctly detected
TEST_F(SkillManifestTest, HasSkillMdDetection) {
  EXPECT_FALSE(SkillManifest::HasSkillMd(test_dir_));
  WriteSkillMd("---\nname: test\n---\n");
  EXPECT_TRUE(SkillManifest::HasSkillMd(test_dir_));
}

// Test: Basic SKILL.md parsing
TEST_F(SkillManifestTest, ParseBasicSkillMd) {
  WriteSkillMd(
      "---\n"
      "name: get_battery_info\n"
      "description: \"Get battery information\"\n"
      "category: Device Info\n"
      "risk_level: low\n"
      "runtime: python\n"
      "---\n\n"
      "# Get Battery Info\n\n"
      "Documentation...\n\n"
      "```json:parameters\n"
      "{\n"
      "  \"type\": \"object\",\n"
      "  \"properties\": {},\n"
      "  \"required\": []\n"
      "}\n"
      "```\n");

  auto j = SkillManifest::Load(test_dir_);
  EXPECT_FALSE(j.empty());
  EXPECT_EQ(j["name"], "get_battery_info");
  EXPECT_EQ(j["description"], "Get battery information");
  EXPECT_EQ(j["category"], "Device Info");
  EXPECT_EQ(j["risk_level"], "low");
  EXPECT_EQ(j["runtime"], "python");
  EXPECT_TRUE(j.contains("parameters"));
  EXPECT_EQ(j["parameters"]["type"], "object");
}

// Test: SKILL.md takes priority over manifest.json
TEST_F(SkillManifestTest, SkillMdPriority) {
  WriteSkillMd(
      "---\n"
      "name: from_skill_md\n"
      "description: \"From SKILL.md\"\n"
      "---\n\n"
      "```json:parameters\n"
      "{\"type\": \"object\", \"properties\": {}}\n"
      "```\n");
  WriteManifestJson({
      {"name", "from_manifest"},
      {"description", "From manifest.json"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()}}}
  });

  auto j = SkillManifest::Load(test_dir_);
  EXPECT_EQ(j["name"], "from_skill_md");
  EXPECT_EQ(j["description"], "From SKILL.md");
}

// Test: Falls back to manifest.json when no SKILL.md
TEST_F(SkillManifestTest, FallbackToManifestJson) {
  WriteManifestJson({
      {"name", "legacy_skill"},
      {"description", "Legacy format"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });

  auto j = SkillManifest::Load(test_dir_);
  EXPECT_EQ(j["name"], "legacy_skill");
}

// Test: Empty directory returns empty JSON
TEST_F(SkillManifestTest, EmptyDirReturnsEmpty) {
  auto j = SkillManifest::Load(test_dir_);
  EXPECT_TRUE(j.empty());
}

// Test: SKILL.md without frontmatter returns empty
TEST_F(SkillManifestTest, NoFrontmatterReturnsEmpty) {
  WriteSkillMd("# Just a heading\nNo frontmatter.\n");
  auto j = SkillManifest::ParseSkillMd(
      test_dir_ + "/SKILL.md");
  EXPECT_TRUE(j.empty());
}

// Test: SKILL.md with default parameters
TEST_F(SkillManifestTest, DefaultParameters) {
  WriteSkillMd(
      "---\n"
      "name: simple_skill\n"
      "description: \"Simple skill\"\n"
      "---\n\n"
      "# Simple Skill\n\n"
      "No parameters block.\n");

  auto j = SkillManifest::Load(test_dir_);
  EXPECT_FALSE(j.empty());
  EXPECT_TRUE(j.contains("parameters"));
  EXPECT_EQ(j["parameters"]["type"], "object");
}

// Test: GenerateSkillMd roundtrip
TEST_F(SkillManifestTest, GenerateSkillMdRoundtrip) {
  nlohmann::json manifest = {
      {"name", "roundtrip_skill"},
      {"description", "Test roundtrip"},
      {"category", "Testing"},
      {"risk_level", "low"},
      {"runtime", "python"},
      {"entry_point", "roundtrip_skill.py"},
      {"parameters",
       {{"type", "object"},
        {"properties",
         {{"query", {{"type", "string"},
                     {"description", "Search query"}}}}},
        {"required", nlohmann::json::array({"query"})}}}
  };

  std::string md =
      SkillManifest::GenerateSkillMd(manifest);

  // Write and re-parse
  WriteSkillMd(md);
  auto j = SkillManifest::ParseSkillMd(
      test_dir_ + "/SKILL.md");
  EXPECT_EQ(j["name"], "roundtrip_skill");
  EXPECT_EQ(j["description"], "Test roundtrip");
  EXPECT_EQ(j["runtime"], "python");
  EXPECT_TRUE(j.contains("parameters"));
  EXPECT_EQ(j["parameters"]["type"], "object");
}

// Test: Verifier works with SKILL.md
TEST_F(SkillManifestTest, VerifierWithSkillMd) {
  WriteSkillMd(
      "---\n"
      "name: test_skill\n"
      "description: \"A test skill\"\n"
      "runtime: python\n"
      "---\n\n"
      "# Test Skill\n\n"
      "```json:parameters\n"
      "{\n"
      "  \"type\": \"object\",\n"
      "  \"properties\": {},\n"
      "  \"required\": []\n"
      "}\n"
      "```\n");
  WriteScript("test_skill.py",
              "import json\n"
              "print(json.dumps({\"status\": \"ok\"}))\n");

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_TRUE(result.passed);
  EXPECT_TRUE(result.errors.empty());
}

// Test: Verifier fails on SKILL.md missing name
TEST_F(SkillManifestTest, VerifierFailsSkillMdNoName) {
  WriteSkillMd(
      "---\n"
      "description: \"No name skill\"\n"
      "---\n\n"
      "```json:parameters\n"
      "{\"type\": \"object\", \"properties\": {}}\n"
      "```\n");

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  bool found = false;
  for (const auto& e : result.errors) {
    if (e.find("name") != std::string::npos) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found);
}

// Test: Disable/Enable with SKILL.md
TEST_F(SkillManifestTest, DisableEnableWithSkillMd) {
  WriteSkillMd(
      "---\n"
      "name: test_skill\n"
      "description: \"Test\"\n"
      "verified: true\n"
      "---\n\n"
      "```json:parameters\n"
      "{\"type\": \"object\", \"properties\": {}}\n"
      "```\n");

  // Disable
  SkillVerifier::DisableSkill(test_dir_);
  {
    auto j = SkillManifest::ParseSkillMd(
        test_dir_ + "/SKILL.md");
    EXPECT_FALSE(j.empty());
    EXPECT_TRUE(j.contains("verified"));
    // GenerateSkillMd writes "false" as string
    // since frontmatter is text-based
    EXPECT_EQ(j["verified"], "false");
  }

  // Enable
  SkillVerifier::EnableSkill(test_dir_);
  {
    auto j = SkillManifest::ParseSkillMd(
        test_dir_ + "/SKILL.md");
    EXPECT_FALSE(j.empty());
    EXPECT_TRUE(j.contains("verified"));
    EXPECT_EQ(j["verified"], "true");
  }
}
