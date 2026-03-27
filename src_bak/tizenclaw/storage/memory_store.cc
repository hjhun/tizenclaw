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
#include "memory_store.hh"

#include <algorithm>
#include <chrono>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace fs = std::filesystem;

namespace {

constexpr size_t kMaxResultLen = 200;

}  // namespace

MemoryStore::MemoryStore()
    : base_dir_("/opt/usr/share/tizenclaw/memory") {}

void MemoryStore::SetDirectory(const std::string& dir) {
  base_dir_ = dir;
}

bool MemoryStore::LoadConfig(const std::string& path) {
  std::ifstream f(path);
  if (!f.is_open()) {
    LOG(WARNING) << "Memory config not found: "
                 << path << " (using defaults)";
    return false;
  }

  try {
    nlohmann::json cfg;
    f >> cfg;

    if (cfg.contains("short_term")) {
      auto& st = cfg["short_term"];
      config_.short_term_max_age_hours =
          st.value("max_age_hours", 24);
      config_.short_term_max_entries =
          st.value("max_entries_per_session", 50);
    }
    if (cfg.contains("long_term")) {
      config_.long_term_max_file_bytes =
          cfg["long_term"].value(
              "max_file_size_bytes", 2048);
    }
    if (cfg.contains("episodic")) {
      auto& ep = cfg["episodic"];
      config_.episodic_max_age_days =
          ep.value("max_age_days", 30);
      config_.episodic_max_file_bytes =
          ep.value("max_file_size_bytes", 2048);
    }
    if (cfg.contains("summary")) {
      auto& su = cfg["summary"];
      config_.summary_max_bytes =
          su.value("max_size_bytes", 8192);
      config_.summary_recent_activity =
          su.value("recent_activity_count", 5);
      config_.summary_recent_episodic =
          su.value("recent_episodic_count", 10);
    }

    LOG(INFO) << "Memory config loaded ("
              << "short_term.max_age_hours="
              << config_.short_term_max_age_hours
              << ", episodic.max_age_days="
              << config_.episodic_max_age_days << ")";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse memory config: "
               << e.what();
    return false;
  }
}

// ------------------------------------------------
// Utility helpers
// ------------------------------------------------

std::string MemoryStore::GetTimestamp() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%dT%H:%M:%S%z");
  return oss.str();
}

std::string MemoryStore::GetDatePrefix() {
  auto now = std::chrono::system_clock::now();
  auto t = std::chrono::system_clock::to_time_t(now);
  std::tm tm_buf{};
  localtime_r(&t, &tm_buf);
  std::ostringstream oss;
  oss << std::put_time(&tm_buf, "%Y-%m-%d");
  return oss.str();
}

void MemoryStore::EnsureDir(const std::string& dir) {
  std::error_code ec;
  fs::create_directories(dir, ec);
}

bool MemoryStore::AtomicWrite(
    const std::string& path,
    const std::string& content) {
  std::string tmp_path = path + ".tmp";
  std::ofstream out(tmp_path);
  if (!out.is_open()) {
    LOG(ERROR) << "Failed to open: " << tmp_path;
    return false;
  }
  out << content;
  out.close();
  if (out.fail()) {
    LOG(ERROR) << "Write failed: " << tmp_path;
    std::error_code ec;
    fs::remove(tmp_path, ec);
    return false;
  }
  std::error_code ec;
  fs::rename(tmp_path, path, ec);
  if (ec) {
    LOG(ERROR) << "Rename failed: " << tmp_path
               << " -> " << path;
    fs::remove(tmp_path, ec);
    return false;
  }
  return true;
}

std::string MemoryStore::TypeToString(MemoryType type) {
  switch (type) {
    case MemoryType::kShortTerm: return "short-term";
    case MemoryType::kLongTerm: return "long-term";
    case MemoryType::kEpisodic: return "episodic";
  }
  return "unknown";
}

std::string MemoryStore::GetTypeDir(
    MemoryType type) const {
  return base_dir_ + "/" + TypeToString(type);
}

std::string MemoryStore::GetSessionShortTermDir(
    const std::string& session_id) const {
  return base_dir_ + "/short-term/" + session_id;
}

// ------------------------------------------------
// Markdown serialization
// ------------------------------------------------

std::string MemoryStore::EntryToMarkdown(
    const MemoryEntry& entry) {
  std::ostringstream md;

  md << "---\n";
  md << "type: " << TypeToString(entry.type) << "\n";
  md << "created: " << entry.created << "\n";
  md << "updated: " << entry.updated << "\n";
  if (!entry.tags.empty()) {
    md << "tags: [";
    for (size_t i = 0; i < entry.tags.size(); ++i) {
      if (i > 0) md << ", ";
      md << entry.tags[i];
    }
    md << "]\n";
  }
  if (!entry.importance.empty())
    md << "importance: " << entry.importance << "\n";
  md << "---\n\n";

  if (!entry.title.empty())
    md << "# " << entry.title << "\n\n";

  md << entry.content << "\n";
  return md.str();
}

std::optional<MemoryEntry> MemoryStore::MarkdownToEntry(
    const std::string& content, MemoryType type) {
  MemoryEntry entry;
  entry.type = type;

  // Parse YAML frontmatter
  if (content.substr(0, 4) != "---\n") return std::nullopt;

  auto end_pos = content.find("\n---\n", 4);
  if (end_pos == std::string::npos) return std::nullopt;

  std::string frontmatter =
      content.substr(4, end_pos - 4);
  std::string body = content.substr(end_pos + 5);

  // Parse frontmatter fields
  std::istringstream fm_stream(frontmatter);
  std::string line;
  while (std::getline(fm_stream, line)) {
    if (line.find("created: ") == 0)
      entry.created = line.substr(9);
    else if (line.find("updated: ") == 0)
      entry.updated = line.substr(9);
    else if (line.find("importance: ") == 0)
      entry.importance = line.substr(12);
    else if (line.find("tags: [") == 0) {
      // Parse [tag1, tag2, tag3]
      auto start = line.find('[');
      auto end = line.find(']');
      if (start != std::string::npos &&
          end != std::string::npos) {
        std::string tags_str =
            line.substr(start + 1, end - start - 1);
        std::istringstream ts(tags_str);
        std::string tag;
        while (std::getline(ts, tag, ',')) {
          // Trim whitespace
          auto first = tag.find_first_not_of(" ");
          if (first != std::string::npos)
            tag = tag.substr(first);
          auto last = tag.find_last_not_of(" ");
          if (last != std::string::npos)
            tag = tag.substr(0, last + 1);
          if (!tag.empty())
            entry.tags.push_back(tag);
        }
      }
    }
  }

  // Parse body: title from # heading
  auto title_start = body.find("# ");
  if (title_start != std::string::npos) {
    auto title_end = body.find('\n', title_start);
    entry.title =
        body.substr(title_start + 2,
                    title_end - title_start - 2);
    if (title_end != std::string::npos)
      body = body.substr(title_end + 1);
  }

  // Trim leading/trailing whitespace from body
  auto first = body.find_first_not_of(" \n\r\t");
  if (first != std::string::npos)
    body = body.substr(first);
  auto last = body.find_last_not_of(" \n\r\t");
  if (last != std::string::npos)
    body = body.substr(0, last + 1);

  entry.content = body;
  return entry;
}

// ------------------------------------------------
// CRUD Operations
// ------------------------------------------------

bool MemoryStore::WriteMemory(
    const MemoryEntry& entry) {
  if (entry.title.empty()) {
    LOG(ERROR) << "MemoryStore: title is required";
    return false;
  }

  std::string dir = GetTypeDir(entry.type);
  EnsureDir(dir);

  // Fill timestamps if missing
  MemoryEntry e = entry;
  std::string now = GetTimestamp();
  if (e.created.empty()) e.created = now;
  e.updated = now;

  std::string filename =
      GetDatePrefix() + "-" + e.title + ".md";
  std::string path = dir + "/" + filename;

  std::string content = EntryToMarkdown(e);

  // Enforce size limit for long-term and episodic
  if (e.type != MemoryType::kShortTerm) {
    int max_bytes = (e.type == MemoryType::kLongTerm)
                        ? config_.long_term_max_file_bytes
                        : config_.episodic_max_file_bytes;
    if (static_cast<int>(content.size()) > max_bytes) {
      LOG(WARNING) << "Memory entry too large ("
                   << content.size() << " > "
                   << max_bytes << "), truncating";
      // Truncate content body to fit
      int overhead = static_cast<int>(
          content.size() - e.content.size());
      int max_body = max_bytes - overhead;
      if (max_body > 0) {
        e.content = e.content.substr(0, max_body);
        content = EntryToMarkdown(e);
      }
    }
  }

  if (!AtomicWrite(path, content)) {
    LOG(ERROR) << "Failed to write memory: " << path;
    return false;
  }

  // Mark summary dirty for non-short-term writes
  if (e.type != MemoryType::kShortTerm)
    summary_dirty_.store(true);

  LOG(DEBUG) << "Memory written: " << filename;
  return true;
}

std::optional<MemoryEntry> MemoryStore::ReadMemory(
    MemoryType type,
    const std::string& filename) const {
  std::string path =
      GetTypeDir(type) + "/" + filename;
  std::ifstream in(path);
  if (!in.is_open()) return std::nullopt;

  std::string content(
      (std::istreambuf_iterator<char>(in)),
      std::istreambuf_iterator<char>());
  in.close();

  return MarkdownToEntry(content, type);
}

std::vector<std::string> MemoryStore::ListMemories(
    MemoryType type) const {
  std::vector<std::string> result;
  std::string dir = GetTypeDir(type);
  std::error_code ec;

  if (!fs::is_directory(dir, ec)) return result;

  for (const auto& entry :
       fs::directory_iterator(dir, ec)) {
    if (!entry.is_regular_file(ec)) continue;
    if (entry.path().extension() != ".md") continue;
    result.push_back(
        entry.path().filename().string());
  }

  std::ranges::sort(result);
  return result;
}

bool MemoryStore::DeleteMemory(
    MemoryType type, const std::string& filename) {
  std::string path =
      GetTypeDir(type) + "/" + filename;
  std::error_code ec;
  if (!fs::remove(path, ec)) {
    LOG(WARNING) << "Memory not found: " << path;
    return false;
  }

  if (type != MemoryType::kShortTerm)
    summary_dirty_.store(true);

  LOG(DEBUG) << "Memory deleted: " << filename;
  return true;
}

// ------------------------------------------------
// Summary (memory.md)
// ------------------------------------------------

bool MemoryStore::IsSummaryDirty() const {
  return summary_dirty_.load();
}

void MemoryStore::RegenerateSummary() {
  EnsureDir(base_dir_);

  std::ostringstream md;

  // Count entries
  auto lt_files = ListMemories(MemoryType::kLongTerm);
  auto ep_files = ListMemories(MemoryType::kEpisodic);

  // YAML frontmatter
  md << "---\n";
  md << "updated: " << GetTimestamp() << "\n";
  md << "long_term_count: " << lt_files.size() << "\n";
  md << "episodic_count: " << ep_files.size() << "\n";
  md << "---\n\n";
  md << "# TizenClaw Memory Summary\n\n";

  // --- Recent Activity (Short-term) ---
  md << "## Recent Activity (Short-term)\n\n";
  md << "| Time | Session | Command | Status |\n";
  md << "|------|---------|---------|--------|\n";

  // Scan all session dirs in short-term/
  std::string st_dir = GetTypeDir(MemoryType::kShortTerm);
  std::error_code ec;
  struct RecentEntry {
    std::string time;
    std::string session;
    std::string command;
    std::string status;
    std::string filename;  // for sorting by recency
  };
  std::vector<RecentEntry> recent;

  if (fs::is_directory(st_dir, ec)) {
    for (const auto& sess_dir :
         fs::directory_iterator(st_dir, ec)) {
      if (!sess_dir.is_directory(ec)) continue;
      std::string sid =
          sess_dir.path().filename().string();

      for (const auto& f :
           fs::directory_iterator(sess_dir.path(), ec)) {
        if (!f.is_regular_file(ec)) continue;
        if (f.path().extension() != ".md") continue;

        auto entry = ReadMemory(
            MemoryType::kShortTerm,
            sid + "/" + f.path().filename().string());
        if (!entry) continue;

        RecentEntry re;
        re.filename = f.path().filename().string();
        re.session = sid.size() > 12
                         ? sid.substr(0, 12) + ".."
                         : sid;
        re.command = entry->title;
        re.status = (entry->importance == "success")
                        ? "✅" : "❌";
        // Extract time from timestamp
        auto t_pos = entry->created.find('T');
        if (t_pos != std::string::npos)
          re.time = entry->created.substr(
              t_pos + 1, 5);
        else
          re.time = entry->created;

        recent.push_back(re);
      }
    }
  }

  // Sort by filename (date-prefixed) descending
  std::ranges::sort(recent,
      [](const RecentEntry& a, const RecentEntry& b) {
        return a.filename > b.filename;
      });

  // Take only recent N
  int count = std::min(
      static_cast<int>(recent.size()),
      config_.summary_recent_activity);
  for (int i = 0; i < count; ++i) {
    md << "| " << recent[i].time
       << " | " << recent[i].session
       << " | " << recent[i].command
       << " | " << recent[i].status << " |\n";
  }

  if (recent.empty())
    md << "| - | - | No recent activity | - |\n";

  md << "\n";

  // --- Long-term Memory ---
  md << "## Long-term Memory\n\n";
  for (const auto& f : lt_files) {
    auto entry =
        ReadMemory(MemoryType::kLongTerm, f);
    if (!entry) continue;

    md << "### " << entry->title << "\n";
    // First line of content as summary
    auto nl = entry->content.find('\n');
    std::string summary =
        (nl != std::string::npos)
            ? entry->content.substr(0, nl)
            : entry->content;
    if (summary.size() > 100)
      summary = summary.substr(0, 100) + "...";
    md << "- " << summary << "\n";
    md << "- 참조: [상세]("
       << TypeToString(MemoryType::kLongTerm)
       << "/" << f << ")\n\n";
  }

  if (lt_files.empty())
    md << "No long-term memories stored.\n\n";

  // --- Episodic Memory (Recent N) ---
  md << "## Episodic Memory (Recent)\n\n";
  md << "| Date | Event | Result | Ref |\n";
  md << "|------|-------|--------|-----|\n";

  // Episodic files are date-prefixed, sort desc
  auto ep_sorted = ep_files;
  std::ranges::sort(ep_sorted, std::greater{});

  int ep_count = std::min(
      static_cast<int>(ep_sorted.size()),
      config_.summary_recent_episodic);
  for (int i = 0; i < ep_count; ++i) {
    auto entry =
        ReadMemory(MemoryType::kEpisodic, ep_sorted[i]);
    if (!entry) continue;

    // Extract date from filename
    std::string date =
        ep_sorted[i].substr(0, 10);
    std::string status =
        (entry->importance == "success")
            ? "✅" : "❌";

    md << "| " << date
       << " | " << entry->title
       << " | " << status
       << " | [상세]("
       << TypeToString(MemoryType::kEpisodic)
       << "/" << ep_sorted[i] << ") |\n";
  }

  if (ep_sorted.empty())
    md << "| - | No episodic memories | - | - |\n";

  md << "\n";

  // Write memory.md
  std::string summary = md.str();

  // Enforce summary size limit
  if (static_cast<int>(summary.size()) >
      config_.summary_max_bytes) {
    summary = summary.substr(
        0, config_.summary_max_bytes);
    LOG(WARNING) << "Memory summary truncated to "
                 << config_.summary_max_bytes
                 << " bytes";
  }

  std::string path = base_dir_ + "/memory.md";
  AtomicWrite(path, summary);

  summary_dirty_.store(false);
  LOG(DEBUG) << "Memory summary regenerated ("
             << summary.size() << " bytes)";
}

std::string MemoryStore::LoadSummary() {
  // Auto-regenerate if dirty
  if (summary_dirty_.load())
    RegenerateSummary();

  std::string path = base_dir_ + "/memory.md";
  std::ifstream in(path);
  if (!in.is_open()) return "";

  std::string content(
      (std::istreambuf_iterator<char>(in)),
      std::istreambuf_iterator<char>());
  in.close();
  return content;
}

// ------------------------------------------------
// Short-term (session-scoped)
// ------------------------------------------------

void MemoryStore::RecordCommand(
    const std::string& session_id,
    const std::string& command,
    const std::string& result, bool success) {
  std::string dir =
      GetSessionShortTermDir(session_id);
  EnsureDir(dir);

  // Check entry count limit
  std::error_code ec;
  int count = 0;
  if (fs::is_directory(dir, ec)) {
    for (const auto& _ :
         fs::directory_iterator(dir, ec)) {
      (void)_;
      ++count;
    }
  }

  if (count >= config_.short_term_max_entries) {
    // Remove oldest entry
    std::vector<std::string> files;
    for (const auto& f :
         fs::directory_iterator(dir, ec)) {
      if (f.is_regular_file(ec))
        files.push_back(f.path().string());
    }
    if (!files.empty()) {
      std::ranges::sort(files);
      fs::remove(files.front(), ec);
    }
  }

  // Truncate result for storage efficiency
  std::string short_result = result;
  if (short_result.size() > kMaxResultLen)
    short_result =
        short_result.substr(0, kMaxResultLen) + "...";

  MemoryEntry entry;
  entry.type = MemoryType::kShortTerm;
  entry.title = command;
  entry.content = short_result;
  entry.importance = success ? "success" : "failure";

  std::string now = GetTimestamp();
  entry.created = now;
  entry.updated = now;

  std::string filename =
      GetDatePrefix() + "-" + command + ".md";
  // Sanitize filename
  std::ranges::replace(filename, ' ', '-');
  std::ranges::replace(filename, '/', '_');

  std::string path = dir + "/" + filename;
  AtomicWrite(path, EntryToMarkdown(entry));

  // Short-term changes also dirty the summary
  summary_dirty_.store(true);
}

// ------------------------------------------------
// Episodic (auto skill tracking)
// ------------------------------------------------

void MemoryStore::RecordSkillExecution(
    const std::string& skill_name,
    const nlohmann::json& args,
    const std::string& result,
    bool success, int duration_ms,
    const std::string& context) {
  // Build concise args summary (keys only)
  std::string args_summary;
  if (args.is_object()) {
    for (auto it = args.begin();
         it != args.end(); ++it) {
      if (!args_summary.empty())
        args_summary += ", ";
      args_summary += it.key();
    }
  }

  // Truncate result
  std::string short_result = result;
  if (short_result.size() > kMaxResultLen)
    short_result =
        short_result.substr(0, kMaxResultLen) + "...";

  MemoryEntry entry;
  entry.type = MemoryType::kEpisodic;
  entry.title = skill_name;
  entry.tags = {skill_name};
  entry.importance = success ? "success" : "failure";

  std::ostringstream body;
  body << "## Execution\n";
  body << "- Skill: " << skill_name << "\n";
  if (!args_summary.empty())
    body << "- Args: " << args_summary << "\n";
  body << "- Duration: " << duration_ms << "ms\n";
  body << "- Result: "
       << (success ? "Success" : "Failed") << "\n";
  if (!short_result.empty())
    body << "- Output: " << short_result << "\n";
  if (!context.empty())
    body << "\n## Context\n" << context << "\n";

  entry.content = body.str();

  WriteMemory(entry);
}

// ------------------------------------------------
// Garbage Collection
// ------------------------------------------------

int MemoryStore::PruneShortTerm() {
  int pruned = 0;
  std::string st_dir =
      GetTypeDir(MemoryType::kShortTerm);
  std::error_code ec;

  if (!fs::is_directory(st_dir, ec)) return 0;

  auto now = std::chrono::system_clock::now();
  auto max_age = std::chrono::hours(
      config_.short_term_max_age_hours);

  for (const auto& sess_dir :
       fs::directory_iterator(st_dir, ec)) {
    if (!sess_dir.is_directory(ec)) continue;

    for (const auto& f :
         fs::directory_iterator(
             sess_dir.path(), ec)) {
      if (!f.is_regular_file(ec)) continue;

      auto ftime = f.last_write_time(ec);
      if (ec) continue;

      // Convert file_time_type to system_clock
      auto sctp =
          std::chrono::time_point_cast<
              std::chrono::system_clock::duration>(
              ftime - fs::file_time_type::clock::now() +
              now);
      if (now - sctp > max_age) {
        fs::remove(f.path(), ec);
        ++pruned;
      }
    }

    // Remove empty session dirs
    if (fs::is_empty(sess_dir.path(), ec))
      fs::remove(sess_dir.path(), ec);
  }

  if (pruned > 0) {
    summary_dirty_.store(true);
    LOG(INFO) << "Pruned " << pruned
              << " short-term entries";
  }
  return pruned;
}

int MemoryStore::PruneEpisodic() {
  int pruned = 0;
  std::string ep_dir =
      GetTypeDir(MemoryType::kEpisodic);
  std::error_code ec;

  if (!fs::is_directory(ep_dir, ec)) return 0;

  auto now = std::chrono::system_clock::now();
  auto max_age = std::chrono::hours(
      config_.episodic_max_age_days * 24);

  for (const auto& f :
       fs::directory_iterator(ep_dir, ec)) {
    if (!f.is_regular_file(ec)) continue;

    auto ftime = f.last_write_time(ec);
    if (ec) continue;

    auto sctp =
        std::chrono::time_point_cast<
            std::chrono::system_clock::duration>(
            ftime - fs::file_time_type::clock::now() +
            now);
    if (now - sctp > max_age) {
      fs::remove(f.path(), ec);
      ++pruned;
    }
  }

  if (pruned > 0) {
    summary_dirty_.store(true);
    LOG(INFO) << "Pruned " << pruned
              << " episodic entries";
  }
  return pruned;
}

}  // namespace tizenclaw
