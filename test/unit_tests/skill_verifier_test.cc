#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>

#include <json.hpp>

#include "skill_verifier.hh"

using namespace tizenclaw;

class SkillVerifierTest : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    namespace fs = std::filesystem;
    test_dir_ = fs::absolute(
        std::string("/tmp/test_skill_verifier_") + test_name).string();
    fs::create_directories(test_dir_);
  }

  void TearDown() override {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::remove_all(test_dir_, ec);
  }

  void WriteManifest(const nlohmann::json& j) {
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

  void WriteExecutable(const std::string& name,
                       const std::string& content) {
    std::string path = test_dir_ + "/" + name;
    std::ofstream f(path);
    f << content;
    f.close();
    chmod(path.c_str(), 0755);
  }

  std::string test_dir_;
};

TEST_F(SkillVerifierTest, IsValidRuntime) {
  EXPECT_TRUE(SkillVerifier::IsValidRuntime("python"));
  EXPECT_TRUE(SkillVerifier::IsValidRuntime("node"));
  EXPECT_TRUE(SkillVerifier::IsValidRuntime("native"));
  EXPECT_FALSE(SkillVerifier::IsValidRuntime("ruby"));
  EXPECT_FALSE(SkillVerifier::IsValidRuntime(""));
}

TEST_F(SkillVerifierTest, ValidPythonManifestPasses) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "A test skill"},
      {"runtime", "python"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });
  WriteScript("test_skill.py",
              "import json\n"
              "print(json.dumps({\"status\": \"ok\"}))\n");

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_TRUE(result.passed);
  EXPECT_TRUE(result.errors.empty());
}

TEST_F(SkillVerifierTest, ValidNodeManifestPasses) {
  WriteManifest({
      {"name", "test_node"},
      {"description", "A test node skill"},
      {"runtime", "node"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });
  WriteScript("test_node.js",
              "console.log(JSON.stringify("
              "{status: 'ok'}));\n");

  auto result = SkillVerifier::Verify(test_dir_);
  // May pass or warn depending on node availability
  // At minimum, manifest and entry point should be valid
  if (access("/usr/bin/node", X_OK) == 0) {
    EXPECT_TRUE(result.passed);
  }
}

TEST_F(SkillVerifierTest, MissingManifestFails) {
  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  ASSERT_FALSE(result.errors.empty());
  EXPECT_NE(result.errors[0].find("manifest.json"),
            std::string::npos);
}

TEST_F(SkillVerifierTest, MissingNameFails) {
  WriteManifest({
      {"description", "No name"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });

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

TEST_F(SkillVerifierTest, InvalidRuntimeFails) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "Invalid runtime"},
      {"runtime", "ruby"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  bool found = false;
  for (const auto& e : result.errors) {
    if (e.find("runtime") != std::string::npos) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found);
}

TEST_F(SkillVerifierTest, MissingParametersFails) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "No params"},
  });

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  bool found = false;
  for (const auto& e : result.errors) {
    if (e.find("parameters") != std::string::npos) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found);
}

TEST_F(SkillVerifierTest, MissingEntryPointFails) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "Missing script"},
      {"runtime", "python"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });
  // No skill file created

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  bool found = false;
  for (const auto& e : result.errors) {
    if (e.find("Entry point") != std::string::npos ||
        e.find("not found") != std::string::npos) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found);
}

TEST_F(SkillVerifierTest, NativeWithoutExecPermFails) {
  WriteManifest({
      {"name", "test_native"},
      {"description", "No exec perm"},
      {"runtime", "native"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });
  // Write file without +x
  WriteScript("test_native", "#!/bin/sh\necho ok\n");

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_FALSE(result.passed);
  bool found = false;
  for (const auto& e : result.errors) {
    if (e.find("execute") != std::string::npos ||
        e.find("permission") != std::string::npos) {
      found = true;
      break;
    }
  }
  EXPECT_TRUE(found);
}

TEST_F(SkillVerifierTest, DisableAndEnableSkill) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "Test"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}},
      {"verified", true}
  });

  // Disable
  SkillVerifier::DisableSkill(test_dir_);
  {
    std::ifstream f(test_dir_ + "/manifest.json");
    nlohmann::json j;
    f >> j;
    EXPECT_FALSE(j["verified"].get<bool>());
  }

  // Enable
  SkillVerifier::EnableSkill(test_dir_);
  {
    std::ifstream f(test_dir_ + "/manifest.json");
    nlohmann::json j;
    f >> j;
    EXPECT_TRUE(j["verified"].get<bool>());
  }
}

TEST_F(SkillVerifierTest, LanguageWarningForNonNative) {
  WriteManifest({
      {"name", "test_skill"},
      {"description", "Language on python"},
      {"runtime", "python"},
      {"language", "cpp"},
      {"parameters", {{"type", "object"},
                      {"properties", nlohmann::json::object()},
                      {"required", nlohmann::json::array()}}}
  });
  WriteScript("test_skill.py",
              "import json\n"
              "print(json.dumps({\"status\": \"ok\"}))\n");

  auto result = SkillVerifier::Verify(test_dir_);
  EXPECT_TRUE(result.passed);
  // Should have warning about language
  bool found_warning = false;
  for (const auto& w : result.warnings) {
    if (w.find("language") != std::string::npos) {
      found_warning = true;
      break;
    }
  }
  EXPECT_TRUE(found_warning);
}
