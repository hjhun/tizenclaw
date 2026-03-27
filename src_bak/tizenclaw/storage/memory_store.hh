/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#ifndef MEMORY_STORE_HH_
#define MEMORY_STORE_HH_

#include <json.hpp>
#include <atomic>
#include <optional>
#include <string>
#include <vector>

namespace tizenclaw {

enum class MemoryType { kShortTerm, kLongTerm, kEpisodic };

struct MemoryEntry {
  MemoryType type;
  std::string title;
  std::string content;
  std::vector<std::string> tags;
  std::string importance;  // "low", "medium", "high"
  std::string created;
  std::string updated;
};

// Configuration loaded from memory_config.json
struct MemoryConfig {
  int short_term_max_age_hours = 24;
  int short_term_max_entries = 50;
  int long_term_max_file_bytes = 2048;
  int episodic_max_age_days = 30;
  int episodic_max_file_bytes = 2048;
  int summary_max_bytes = 8192;
  int summary_recent_activity = 5;
  int summary_recent_episodic = 10;
};

class MemoryStore {
 public:
  MemoryStore();

  // Set custom base directory (for tests)
  void SetDirectory(const std::string& dir);

  // Load config from memory_config.json
  [[nodiscard]] bool LoadConfig(const std::string& path);

  // Access current config
  const MemoryConfig& GetConfig() const { return config_; }

  // --- CRUD ---

  // Write a memory entry to disk
  [[nodiscard]] bool WriteMemory(const MemoryEntry& entry);

  // Read a specific memory file
  [[nodiscard]] std::optional<MemoryEntry> ReadMemory(
      MemoryType type, const std::string& filename) const;

  // List memory filenames of a given type
  [[nodiscard]] std::vector<std::string> ListMemories(
      MemoryType type) const;

  // Delete a memory entry
  [[nodiscard]] bool DeleteMemory(MemoryType type,
                                  const std::string& filename);

  // --- Summary (dirty flag based) ---

  // Regenerate memory.md from current files
  void RegenerateSummary();

  // Load memory.md (auto-regenerates if dirty)
  [[nodiscard]] std::string LoadSummary();

  // Check if summary needs regeneration
  [[nodiscard]] bool IsSummaryDirty() const;

  // --- Short-term (session-scoped) ---

  // Record a command result to session's
  // short-term memory
  void RecordCommand(const std::string& session_id,
                     const std::string& command,
                     const std::string& result,
                     bool success);

  // --- Episodic (auto skill tracking) ---

  // Record a skill execution result
  void RecordSkillExecution(
      const std::string& skill_name,
      const nlohmann::json& args,
      const std::string& result,
      bool success, int duration_ms,
      const std::string& context = "");

  // --- Garbage Collection ---

  // Prune short-term entries older than config
  [[nodiscard]] int PruneShortTerm();

  // Prune episodic entries older than config
  [[nodiscard]] int PruneEpisodic();

  // Convert MemoryType to string
  static std::string TypeToString(MemoryType type);

 private:
  // Get directory path for a memory type
  std::string GetTypeDir(MemoryType type) const;

  // Get directory for session short-term
  std::string GetSessionShortTermDir(
      const std::string& session_id) const;

  // Format a MemoryEntry as Markdown with
  // YAML frontmatter
  static std::string EntryToMarkdown(
      const MemoryEntry& entry);

  // Parse a Markdown file into a MemoryEntry
  static std::optional<MemoryEntry> MarkdownToEntry(
      const std::string& content, MemoryType type);

  // Get current timestamp as ISO string
  static std::string GetTimestamp();

  // Get current date as YYYY-MM-DD
  static std::string GetDatePrefix();

  // Ensure a directory exists (mkdir -p)
  static void EnsureDir(const std::string& dir);

  // Atomic file write (write .tmp then rename)
  static bool AtomicWrite(const std::string& path,
                          const std::string& content);

  std::string base_dir_;
  MemoryConfig config_;
  std::atomic<bool> summary_dirty_{false};
};

}  // namespace tizenclaw

#endif  // MEMORY_STORE_HH_
