#include <gtest/gtest.h>
#include <sys/stat.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>

#include "system_cli_adapter.hh"
#include "capability_registry.hh"

using namespace tizenclaw;

class SystemCliAdapterTest : public ::testing::Test {
 protected:
  void SetUp() override {
    namespace fs = std::filesystem;
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    test_dir_ = fs::absolute(
        std::string("/tmp/test_sys_cli_") + test_name).string();
    tools_dir_ = test_dir_ + "/tools/system_cli";
    config_path_ = test_dir_ + "/config.json";

    fs::create_directories(tools_dir_);

    CapabilityRegistry::GetInstance().Clear();
  }

  void TearDown() override {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::remove_all(test_dir_, ec);

    SystemCliAdapter::GetInstance().Shutdown();
    CapabilityRegistry::GetInstance().Clear();
  }

  void WriteConfig(const std::string& content) {
    std::ofstream f(config_path_);
    f << content;
    f.close();
  }

  void WriteToolDoc(const std::string& tool_name,
                    const std::string& content) {
    std::ofstream f(tools_dir_ + "/" + tool_name + ".tool.md");
    f << content;
    f.close();
  }

  // Create a fake binary for testing
  void CreateFakeBinary(const std::string& path) {
    namespace fs = std::filesystem;
    std::error_code ec;
    fs::create_directories(
        fs::path(path).parent_path(), ec);
    std::ofstream f(path);
    f << "#!/bin/sh\necho test";
    f.close();
    chmod(path.c_str(), 0755);
  }

  std::string test_dir_;
  std::string tools_dir_;
  std::string config_path_;
};

TEST_F(SystemCliAdapterTest, InitEmptyConfig) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_TRUE(adapter.IsEnabled());
  EXPECT_TRUE(adapter.GetToolNames().empty());
}

TEST_F(SystemCliAdapterTest, InitDisabled) {
  WriteConfig(R"({"enabled": false, "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_FALSE(adapter.IsEnabled());
}

TEST_F(SystemCliAdapterTest, InitMissingConfig) {
  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize("/nonexistent/config.json"));
  EXPECT_FALSE(adapter.IsEnabled());
}

TEST_F(SystemCliAdapterTest, InitMalformedConfig) {
  WriteConfig("{invalid json}}}");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_FALSE(adapter.Initialize(config_path_));
}

TEST_F(SystemCliAdapterTest, RegisterToolWithBinary) {
  std::string fake_bin = test_dir_ + "/bin/my_tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"my_tool": {"path": ")" +
      fake_bin + R"(", "timeout_seconds": 5, "side_effect": "none",
      "description": "A test tool"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_TRUE(adapter.HasTool("my_tool"));
  EXPECT_EQ(adapter.Resolve("my_tool"), fake_bin);
  EXPECT_EQ(adapter.GetTimeout("my_tool"), 5);
}

TEST_F(SystemCliAdapterTest, SkipMissingBinary) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"missing_tool": {
      "path": "/nonexistent/binary",
      "description": "Should be skipped"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_FALSE(adapter.HasTool("missing_tool"));
}

TEST_F(SystemCliAdapterTest, HasToolFalse) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);
  EXPECT_FALSE(adapter.HasTool("nonexistent"));
}

TEST_F(SystemCliAdapterTest, ValidateArguments) {
  std::string fake_bin = test_dir_ + "/bin/tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"tool": {"path": ")" +
      fake_bin + R"(", "blocked_args": ["--delete", "-kill -all"],
      "description": "Tool with blocked args"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  // Valid arguments
  EXPECT_TRUE(adapter.ValidateArguments("tool", "getallpkg").empty());
  EXPECT_TRUE(adapter.ValidateArguments("tool", "-topwins").empty());

  // Blocked arguments
  EXPECT_FALSE(
      adapter.ValidateArguments("tool", "--delete foo").empty());
  EXPECT_FALSE(
      adapter.ValidateArguments("tool", "-kill -all").empty());
}

TEST_F(SystemCliAdapterTest, ValidateArgumentsUnknownTool) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  EXPECT_FALSE(
      adapter.ValidateArguments("unknown", "args").empty());
}

TEST_F(SystemCliAdapterTest, LoadToolDocs) {
  std::string fake_bin = test_dir_ + "/bin/my_tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"my_tool": {"path": ")" +
      fake_bin + R"(", "description": "Test"}}})");

  WriteToolDoc("my_tool", "# my_tool\nA test tool documentation");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  auto docs = adapter.GetToolDocs();
  EXPECT_EQ(docs.size(), 1u);
  EXPECT_NE(docs.find("my_tool"), docs.end());
  EXPECT_NE(docs["my_tool"].find("test tool"),
             std::string::npos);
}

TEST_F(SystemCliAdapterTest, DocsNotLoadedForNonWhitelistedTool) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  // Write a tool doc for a tool NOT in the whitelist
  WriteToolDoc("unlisted_tool", "# unlisted\nShould not be loaded");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  auto docs = adapter.GetToolDocs();
  EXPECT_TRUE(docs.empty());
}

TEST_F(SystemCliAdapterTest, RegistersCapabilities) {
  std::string fake_bin = test_dir_ + "/bin/tool_a";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"tool_a": {"path": ")" +
      fake_bin + R"(", "side_effect": "reversible",
      "description": "Tool A"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  auto* cap = CapabilityRegistry::GetInstance().Get("system_cli:tool_a");
  ASSERT_NE(cap, nullptr);
  EXPECT_EQ(cap->category, "system_cli");
  EXPECT_EQ(cap->source, CapabilitySource::kSystemCli);
  EXPECT_EQ(cap->contract.side_effect, SideEffect::kReversible);
}

TEST_F(SystemCliAdapterTest, GetToolNames) {
  std::string bin_a = test_dir_ + "/bin/a";
  std::string bin_b = test_dir_ + "/bin/b";
  CreateFakeBinary(bin_a);
  CreateFakeBinary(bin_b);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {
      "a": {"path": ")" + bin_a + R"(", "description": "A"},
      "b": {"path": ")" + bin_b + R"(", "description": "B"}
      }})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  auto names = adapter.GetToolNames();
  EXPECT_EQ(names.size(), 2u);
}

TEST_F(SystemCliAdapterTest, Shutdown) {
  std::string fake_bin = test_dir_ + "/bin/tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"tool": {"path": ")" +
      fake_bin + R"(", "description": "T"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);
  EXPECT_TRUE(adapter.HasTool("tool"));

  adapter.Shutdown();
  EXPECT_FALSE(adapter.HasTool("tool"));
  EXPECT_FALSE(adapter.IsEnabled());
}

TEST_F(SystemCliAdapterTest, DefaultTimeout) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  // Non-existent tool returns default
  EXPECT_EQ(adapter.GetTimeout("nonexistent"), 10);
}

TEST_F(SystemCliAdapterTest, ScanAutoDiscoverFromServiceFiles) {
  // Create a fake systemd dir with a .service file
  namespace fs = std::filesystem;
  std::string systemd_dir = test_dir_ + "/systemd/system";
  fs::create_directories(systemd_dir);

  // Write a .service file with ExecStart pointing to
  // a non-existent binary (should be auto-discovered)
  std::ofstream sf(systemd_dir + "/my_daemon.service");
  sf << "[Unit]\n"
     << "Description=My Daemon\n\n"
     << "[Service]\n"
     << "ExecStart=/usr/bin/my_cli_tool --arg1\n\n"
     << "[Install]\n"
     << "WantedBy=multi-user.target\n";
  sf.close();

  WriteConfig(R"({"enabled": true, "auto_discover": true, )"
      R"("systemd_dir": ")" + systemd_dir + R"(", )"
      R"("tools_dir": ")" + tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_TRUE(adapter.HasTool("my_cli_tool"));
  EXPECT_EQ(adapter.Resolve("my_cli_tool"),
            "/usr/bin/my_cli_tool");
}

TEST_F(SystemCliAdapterTest, AutoDiscoverSkipsExistingBinary) {
  namespace fs = std::filesystem;
  std::string systemd_dir = test_dir_ + "/systemd/system";
  fs::create_directories(systemd_dir);

  // Create a binary that actually exists
  std::string real_bin = test_dir_ + "/bin/real_daemon";
  CreateFakeBinary(real_bin);

  std::ofstream sf(systemd_dir + "/real.service");
  sf << "[Service]\n"
     << "ExecStart=" << real_bin << " --daemon\n";
  sf.close();

  WriteConfig(R"({"enabled": true, "auto_discover": true, )"
      R"("systemd_dir": ")" + systemd_dir + R"(", )"
      R"("tools_dir": ")" + tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);
  // Should NOT be registered because binary exists
  EXPECT_FALSE(adapter.HasTool("real_daemon"));
}

TEST_F(SystemCliAdapterTest, ConfigOverridesAutoDiscover) {
  namespace fs = std::filesystem;
  std::string systemd_dir = test_dir_ + "/systemd/system";
  fs::create_directories(systemd_dir);

  // Service file references /usr/bin/my_tool
  std::ofstream sf(systemd_dir + "/my.service");
  sf << "[Service]\n"
     << "ExecStart=/usr/bin/my_tool --flag\n";
  sf.close();

  // Config explicitly defines my_tool with custom settings
  std::string fake_bin = test_dir_ + "/bin/my_tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "auto_discover": true, )"
      R"("systemd_dir": ")" + systemd_dir + R"(", )"
      R"("tools_dir": ")" + tools_dir_ + R"(", "tools": {)"
      R"("my_tool": {"path": ")" + fake_bin + R"(", )"
      R"("timeout_seconds": 30, "description": "Config tool"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);
  EXPECT_TRUE(adapter.HasTool("my_tool"));
  // Config path should be used, not the service file path
  EXPECT_EQ(adapter.Resolve("my_tool"), fake_bin);
  EXPECT_EQ(adapter.GetTimeout("my_tool"), 30);
}

TEST_F(SystemCliAdapterTest, AutoDiscoverDisabledByDefault) {
  namespace fs = std::filesystem;
  std::string systemd_dir = test_dir_ + "/systemd/system";
  fs::create_directories(systemd_dir);

  std::ofstream sf(systemd_dir + "/hidden.service");
  sf << "[Service]\n"
     << "ExecStart=/usr/bin/hidden_tool\n";
  sf.close();

  // auto_discover NOT set (defaults to false)
  WriteConfig(R"({"enabled": true, )"
      R"("systemd_dir": ")" + systemd_dir + R"(", )"
      R"("tools_dir": ")" + tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);
  // Should NOT discover because auto_discover is false
  EXPECT_FALSE(adapter.HasTool("hidden_tool"));
}

TEST_F(SystemCliAdapterTest, RegisterToolRuntime) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));

  // Create a fake binary
  std::string fake_bin = test_dir_ + "/bin/new_tool";
  CreateFakeBinary(fake_bin);

  SystemCliToolConfig cfg;
  cfg.binary_path = fake_bin;
  cfg.timeout_seconds = 15;
  cfg.side_effect = "none";
  cfg.description = "Dynamically registered tool";
  cfg.blocked_args = {"--dangerous"};

  std::string tool_doc = "# new_tool\nA new tool\n";
  std::string err = adapter.RegisterTool(
      "new_tool", cfg, tool_doc);
  EXPECT_TRUE(err.empty()) << "Error: " << err;
  EXPECT_TRUE(adapter.HasTool("new_tool"));
  EXPECT_EQ(adapter.Resolve("new_tool"), fake_bin);
  EXPECT_EQ(adapter.GetTimeout("new_tool"), 15);

  // Check tool.md was written
  namespace fs = std::filesystem;
  EXPECT_TRUE(fs::exists(
      tools_dir_ + "/new_tool.tool.md"));

  // Check capability was registered
  auto* cap = CapabilityRegistry::GetInstance()
                  .Get("system_cli:new_tool");
  ASSERT_NE(cap, nullptr);
  EXPECT_EQ(cap->category, "system_cli");
}

TEST_F(SystemCliAdapterTest, UnregisterToolRuntime) {
  std::string fake_bin = test_dir_ + "/bin/tmp_tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"tmp_tool": {"path": ")" +
      fake_bin + R"(", "description": "Temp tool"}}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_TRUE(adapter.HasTool("tmp_tool"));

  std::string err = adapter.UnregisterTool("tmp_tool");
  EXPECT_TRUE(err.empty()) << "Error: " << err;
  EXPECT_FALSE(adapter.HasTool("tmp_tool"));

  // Capability should be unregistered
  auto* cap = CapabilityRegistry::GetInstance()
                  .Get("system_cli:tmp_tool");
  EXPECT_EQ(cap, nullptr);
}

TEST_F(SystemCliAdapterTest, UnregisterNonexistentTool) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  std::string err = adapter.UnregisterTool("ghost");
  EXPECT_FALSE(err.empty());
}

TEST_F(SystemCliAdapterTest, GetRegisteredToolsJson) {
  std::string fake_bin = test_dir_ + "/bin/json_tool";
  CreateFakeBinary(fake_bin);

  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {"json_tool": {"path": ")" +
      fake_bin + R"(", "timeout_seconds": 7,
      "side_effect": "reversible",
      "description": "JSON test tool"}}})");

  WriteToolDoc("json_tool", "# json_tool\nTest");

  auto& adapter = SystemCliAdapter::GetInstance();
  adapter.Initialize(config_path_);

  auto result = adapter.GetRegisteredToolsJson();
  EXPECT_TRUE(result["enabled"].get<bool>());
  EXPECT_EQ(result["tool_count"].get<int>(), 1);
  ASSERT_TRUE(result["tools"].is_array());
  EXPECT_EQ(result["tools"].size(), 1u);

  auto& t = result["tools"][0];
  EXPECT_EQ(t["name"].get<std::string>(),
            "json_tool");
  EXPECT_EQ(t["timeout_seconds"].get<int>(), 7);
  EXPECT_TRUE(t["has_doc"].get<bool>());
}

TEST_F(SystemCliAdapterTest, SaveConfigRoundTrip) {
  WriteConfig(R"({"enabled": true, "tools_dir": ")" +
      tools_dir_ + R"(", "tools": {}})");

  auto& adapter = SystemCliAdapter::GetInstance();
  EXPECT_TRUE(adapter.Initialize(config_path_));

  // Register a tool at runtime
  std::string fake_bin = test_dir_ + "/bin/saved_tool";
  CreateFakeBinary(fake_bin);

  SystemCliToolConfig cfg;
  cfg.binary_path = fake_bin;
  cfg.timeout_seconds = 20;
  cfg.side_effect = "none";
  cfg.description = "Saved via config";

  adapter.RegisterTool("saved_tool", cfg, "");

  // Shutdown and reinitialize from same config
  adapter.Shutdown();
  CapabilityRegistry::GetInstance().Clear();
  EXPECT_TRUE(adapter.Initialize(config_path_));
  EXPECT_TRUE(adapter.HasTool("saved_tool"));
  EXPECT_EQ(adapter.GetTimeout("saved_tool"), 20);
}

