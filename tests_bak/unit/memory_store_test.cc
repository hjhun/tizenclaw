#include <gtest/gtest.h>
#include <dirent.h>
#include <sys/stat.h>

#include <cstdlib>
#include <fstream>

#include "memory_store.hh"

using namespace tizenclaw;

class MemoryStoreTest : public ::testing::Test {
 protected:
  void SetUp() override {
    const char* test_name =
        ::testing::UnitTest::GetInstance()
            ->current_test_info()
            ->name();
    base_dir_ = std::string(
        "/tmp/tizenclaw_test_memory_") + test_name;
    mkdir(base_dir_.c_str(), 0700);
    store_.SetDirectory(base_dir_);
  }

  void TearDown() override {
    int ret = system(
        ("rm -rf " + base_dir_).c_str());
    (void)ret;
  }

  MemoryEntry MakeEntry(
      MemoryType type,
      const std::string& title,
      const std::string& content,
      const std::string& importance = "medium") {
    MemoryEntry e;
    e.type = type;
    e.title = title;
    e.content = content;
    e.importance = importance;
    e.tags = {"test"};
    return e;
  }

  std::string base_dir_;
  MemoryStore store_;
};

TEST_F(MemoryStoreTest, WriteAndReadMemory) {
  auto entry = MakeEntry(
      MemoryType::kLongTerm,
      "user-preferences",
      "Preferred language: Korean");

  EXPECT_TRUE(store_.WriteMemory(entry));

  auto files =
      store_.ListMemories(MemoryType::kLongTerm);
  ASSERT_EQ(files.size(), 1u);

  auto loaded = store_.ReadMemory(
      MemoryType::kLongTerm, files[0]);
  ASSERT_TRUE(loaded.has_value());
  EXPECT_EQ(loaded->title, "user-preferences");
  EXPECT_TRUE(loaded->content.find("Korean") !=
              std::string::npos);
  EXPECT_EQ(loaded->importance, "medium");
  EXPECT_EQ(loaded->tags.size(), 1u);
  EXPECT_EQ(loaded->tags[0], "test");
}

TEST_F(MemoryStoreTest, SessionScopedShortTerm) {
  store_.RecordCommand(
      "sess_a", "wifi_scan", "OK", true);
  store_.RecordCommand(
      "sess_b", "bt_pair", "Failed", false);

  // Verify separate dirs
  namespace fs = std::filesystem;
  std::string dir_a =
      base_dir_ + "/short-term/sess_a";
  std::string dir_b =
      base_dir_ + "/short-term/sess_b";

  EXPECT_TRUE(fs::is_directory(dir_a));
  EXPECT_TRUE(fs::is_directory(dir_b));

  // Each session should have exactly 1 entry
  int count_a = 0, count_b = 0;
  for (const auto& _ :
       fs::directory_iterator(dir_a)) {
    (void)_;
    ++count_a;
  }
  for (const auto& _ :
       fs::directory_iterator(dir_b)) {
    (void)_;
    ++count_b;
  }
  EXPECT_EQ(count_a, 1);
  EXPECT_EQ(count_b, 1);
}

TEST_F(MemoryStoreTest, RegenerateSummary) {
  auto lt = MakeEntry(
      MemoryType::kLongTerm,
      "user-lang",
      "User prefers Korean");
  EXPECT_TRUE(store_.WriteMemory(lt));

  store_.RecordCommand(
      "sess_x", "wifi_scan", "OK", true);

  store_.RegenerateSummary();

  std::string summary = store_.LoadSummary();
  EXPECT_FALSE(summary.empty());

  // Should contain all sections
  EXPECT_TRUE(summary.find(
      "## Recent Activity (Short-term)") !=
      std::string::npos);
  EXPECT_TRUE(summary.find(
      "## Long-term Memory") !=
      std::string::npos);
  EXPECT_TRUE(summary.find(
      "## Episodic Memory") !=
      std::string::npos);
  EXPECT_TRUE(summary.find("user-lang") !=
              std::string::npos);
}

TEST_F(MemoryStoreTest,
       DirtyFlagAndLoadSummary) {
  // Initially not dirty
  EXPECT_FALSE(store_.IsSummaryDirty());

  // Write makes it dirty
  auto entry = MakeEntry(
      MemoryType::kLongTerm,
      "test-fact",
      "Some fact");
  EXPECT_TRUE(store_.WriteMemory(entry));
  EXPECT_TRUE(store_.IsSummaryDirty());

  // LoadSummary auto-regenerates
  std::string summary = store_.LoadSummary();
  EXPECT_FALSE(store_.IsSummaryDirty());
  EXPECT_FALSE(summary.empty());
}

TEST_F(MemoryStoreTest, PruneShortTerm) {
  // Record some commands
  store_.RecordCommand(
      "sess_prune", "cmd1", "ok", true);
  store_.RecordCommand(
      "sess_prune", "cmd2", "ok", true);

  // With default 24h, nothing should be pruned
  int pruned = store_.PruneShortTerm();
  EXPECT_EQ(pruned, 0);
}

TEST_F(MemoryStoreTest, PruneEpisodic) {
  nlohmann::json args = {{"key", "val"}};
  store_.RecordSkillExecution(
      "test_skill", args, "OK", true, 100);

  // With default 30d, nothing should be pruned
  int pruned = store_.PruneEpisodic();
  EXPECT_EQ(pruned, 0);

  // Verify episodic file was created
  auto files =
      store_.ListMemories(MemoryType::kEpisodic);
  EXPECT_EQ(files.size(), 1u);
}

TEST_F(MemoryStoreTest, RecordSkillExecution) {
  nlohmann::json args = {
      {"query", "test"}, {"limit", 5}};
  store_.RecordSkillExecution(
      "wifi_scan", args,
      "Scan completed successfully",
      true, 250, "Low memory context");

  auto files =
      store_.ListMemories(MemoryType::kEpisodic);
  ASSERT_EQ(files.size(), 1u);

  auto entry = store_.ReadMemory(
      MemoryType::kEpisodic, files[0]);
  ASSERT_TRUE(entry.has_value());
  EXPECT_EQ(entry->title, "wifi_scan");
  EXPECT_EQ(entry->importance, "success");
  // Args should be listed as keys only
  EXPECT_TRUE(entry->content.find("query") !=
              std::string::npos);
  EXPECT_TRUE(entry->content.find("250ms") !=
              std::string::npos);
}

TEST_F(MemoryStoreTest,
       DeleteMemoryAndSummaryUpdate) {
  auto entry = MakeEntry(
      MemoryType::kLongTerm,
      "to-delete",
      "Will be deleted");
  EXPECT_TRUE(store_.WriteMemory(entry));

  // Clear dirty flag
  store_.RegenerateSummary();
  EXPECT_FALSE(store_.IsSummaryDirty());

  auto files =
      store_.ListMemories(MemoryType::kLongTerm);
  ASSERT_EQ(files.size(), 1u);

  EXPECT_TRUE(store_.DeleteMemory(
      MemoryType::kLongTerm, files[0]));

  // Should be dirty again
  EXPECT_TRUE(store_.IsSummaryDirty());

  // File should be gone
  files =
      store_.ListMemories(MemoryType::kLongTerm);
  EXPECT_TRUE(files.empty());
}

TEST_F(MemoryStoreTest, ListMemories) {
  EXPECT_TRUE(store_.WriteMemory(MakeEntry(
      MemoryType::kLongTerm,
      "mem-a", "content a")));
  EXPECT_TRUE(store_.WriteMemory(MakeEntry(
      MemoryType::kLongTerm,
      "mem-b", "content b")));
  EXPECT_TRUE(store_.WriteMemory(MakeEntry(
      MemoryType::kEpisodic,
      "ep-1", "episodic 1")));

  auto lt_files =
      store_.ListMemories(MemoryType::kLongTerm);
  auto ep_files =
      store_.ListMemories(MemoryType::kEpisodic);

  EXPECT_EQ(lt_files.size(), 2u);
  EXPECT_EQ(ep_files.size(), 1u);

  // Results should be sorted
  EXPECT_LE(lt_files[0], lt_files[1]);
}

TEST_F(MemoryStoreTest, LoadConfigOverrides) {
  // Write a test config
  std::string config_path =
      base_dir_ + "/test_config.json";
  std::ofstream out(config_path);
  out << R"({
    "short_term": { "max_age_hours": 48 },
    "episodic": { "max_age_days": 60 },
    "summary": { "recent_activity_count": 3 }
  })";
  out.close();

  MemoryStore store2;
  EXPECT_TRUE(store2.LoadConfig(config_path));

  auto cfg = store2.GetConfig();
  EXPECT_EQ(cfg.short_term_max_age_hours, 48);
  EXPECT_EQ(cfg.episodic_max_age_days, 60);
  EXPECT_EQ(cfg.summary_recent_activity, 3);
  // Defaults for unset fields
  EXPECT_EQ(cfg.short_term_max_entries, 50);
  EXPECT_EQ(cfg.summary_max_bytes, 8192);
}
