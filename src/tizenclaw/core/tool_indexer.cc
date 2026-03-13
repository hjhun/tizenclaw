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
#include "tool_indexer.hh"

#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <vector>

#include <json.hpp>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

namespace fs = std::filesystem;

// Extract H1 title from an MD file content.
// Returns the text after "# " on the first H1 line.
std::string ExtractTitle(const std::string& content) {
  std::istringstream stream(content);
  std::string line;
  while (std::getline(stream, line)) {
    if (line.size() > 2 && line[0] == '#' &&
        line[1] == ' ') {
      return line.substr(2);
    }
  }
  return "";
}

// Extract the first non-empty paragraph after the
// H1 title from an MD file content.
std::string ExtractFirstParagraph(
    const std::string& content) {
  std::istringstream stream(content);
  std::string line;
  bool past_title = false;

  while (std::getline(stream, line)) {
    // Skip until we pass the H1 title
    if (!past_title) {
      if (line.size() > 2 && line[0] == '#' &&
          line[1] == ' ') {
        past_title = true;
      }
      continue;
    }

    // Skip blank lines
    if (line.empty()) continue;

    // Skip sub-headers and fences
    if (line[0] == '#' || line[0] == '`' ||
        line[0] == '|') {
      break;
    }

    // Truncate long paragraphs
    if (line.size() > 120)
      return line.substr(0, 117) + "...";
    return line;
  }
  return "";
}

// Read entire file content as string.
std::string ReadFile(const std::string& path) {
  std::ifstream in(path);
  if (!in.is_open()) return "";
  return {std::istreambuf_iterator<char>(in),
          std::istreambuf_iterator<char>()};
}

// Write string to file, creating parent dirs.
void WriteFile(const std::string& path,
               const std::string& content) {
  std::error_code ec;
  fs::create_directories(
      fs::path(path).parent_path(), ec);
  std::ofstream out(path);
  if (!out.is_open()) {
    LOG(WARNING) << "ToolIndexer: Cannot write "
                 << path;
    return;
  }
  out << content;
}

// Scan manifest.json files in skill subdirectories
// and build a category-grouped markdown document.
// Categories are read from the "category" field
// in manifest.json (data-driven, no hardcoded
// category lists).
std::string BuildManifestTable(
    const std::string& dir,
    const std::string& title) {
  std::error_code ec;
  if (!fs::is_directory(dir, ec)) return "";

  struct SkillEntry {
    std::string name;
    std::string category;
    std::string description;
    std::string risk;
  };
  std::vector<SkillEntry> entries;

  for (const auto& entry :
       fs::directory_iterator(dir, ec)) {
    if (!entry.is_directory()) continue;
    auto dirname =
        entry.path().filename().string();
    if (dirname[0] == '.') continue;

    std::string manifest =
        entry.path().string() + "/manifest.json";
    std::ifstream mf(manifest);
    if (!mf.is_open()) continue;

    try {
      nlohmann::json j;
      mf >> j;
      SkillEntry e;
      e.name = j.value("name", dirname);
      e.category =
          j.value("category", "Uncategorized");
      e.description =
          j.value("description", "");
      e.risk = j.value("risk_level", "low");

      if (e.description.size() > 80)
        e.description =
            e.description.substr(0, 77) + "...";

      entries.push_back(std::move(e));
    } catch (...) {
      // Skip malformed manifests
    }
  }

  // Group by category (sorted alphabetically)
  std::map<std::string,
           std::vector<const SkillEntry*>>
      groups;
  for (const auto& e : entries)
    groups[e.category].push_back(&e);

  // Sort entries within each group by name
  for (auto& [cat, vec] : groups) {
    std::sort(vec.begin(), vec.end(),
              [](const SkillEntry* a,
                 const SkillEntry* b) {
                return a->name < b->name;
              });
  }

  std::ostringstream md;
  md << "# " << title << "\n\n";

  if (entries.empty()) {
    md << "_No tools registered._\n";
    return md.str();
  }

  int total = 0;
  for (const auto& [cat, vec] : groups) {
    md << "### " << cat << "\n";
    md << "| Name | Description | Risk |\n";
    md << "|------|-------------|------|\n";
    for (const auto* e : vec) {
      md << "| " << e->name << " | "
         << e->description << " | "
         << e->risk << " |\n";
      total++;
    }
    md << "\n";
  }

  md << "Total: " << total << " tools\n";
  return md.str();
}

// Extract category from **Category**: xxx line
// in markdown content.
std::string ExtractCategory(
    const std::string& content) {
  std::istringstream stream(content);
  std::string line;
  while (std::getline(stream, line)) {
    constexpr auto kPrefix = "**Category**: ";
    constexpr size_t kLen = 14;  // strlen above
    if (line.size() >= kLen &&
        line.substr(0, kLen) == kPrefix)
      return line.substr(kLen);
  }
  return "Uncategorized";
}

// Scan .md files and build a category-grouped
// summary table. Categories are extracted from
// **Category**: lines in each .md file.
std::string BuildMdSummaryTable(
    const std::string& dir,
    const std::string& title) {
  std::error_code ec;
  if (!fs::is_directory(dir, ec)) return "";

  struct MdEntry {
    std::string name;
    std::string category;
    std::string description;
  };
  std::vector<MdEntry> entries;

  for (const auto& entry :
       fs::directory_iterator(dir, ec)) {
    if (!entry.is_regular_file()) continue;
    if (entry.path().extension() != ".md")
      continue;
    if (entry.path().filename() == "index.md")
      continue;

    std::string content =
        ReadFile(entry.path().string());
    if (content.empty()) continue;

    MdEntry e;
    e.name = ExtractTitle(content);
    if (e.name.empty())
      e.name = entry.path().stem().string();
    e.category = ExtractCategory(content);
    e.description =
        ExtractFirstParagraph(content);

    entries.push_back(std::move(e));
  }

  // Group by category
  std::map<std::string,
           std::vector<const MdEntry*>>
      groups;
  for (const auto& e : entries)
    groups[e.category].push_back(&e);

  for (auto& [cat, vec] : groups) {
    std::sort(vec.begin(), vec.end(),
              [](const MdEntry* a,
                 const MdEntry* b) {
                return a->name < b->name;
              });
  }

  std::ostringstream md;
  md << "# " << title << "\n\n";

  if (entries.empty()) {
    md << "_No tools registered._\n";
    return md.str();
  }

  int total = 0;
  for (const auto& [cat, vec] : groups) {
    md << "### " << cat << "\n";
    md << "| Name | Description |\n";
    md << "|------|-------------|\n";
    for (const auto* e : vec) {
      md << "| " << e->name << " | "
         << e->description << " |\n";
      total++;
    }
    md << "\n";
  }

  md << "Total: " << total << " tools\n";
  return md.str();
}

}  // namespace

void ToolIndexer::GenerateSkillsIndex(
    const std::string& skills_dir) {
  std::string content =
      BuildManifestTable(skills_dir, "Skills");
  if (content.empty()) return;
  WriteFile(skills_dir + "/index.md", content);
  LOG(INFO) << "ToolIndexer: Generated "
            << skills_dir << "/index.md";
}

void ToolIndexer::GenerateCustomSkillsIndex(
    const std::string& custom_skills_dir) {
  std::string content = BuildManifestTable(
      custom_skills_dir, "Custom Skills");
  if (content.empty()) return;
  WriteFile(custom_skills_dir + "/index.md",
            content);
  LOG(INFO) << "ToolIndexer: Generated "
            << custom_skills_dir << "/index.md";
}

void ToolIndexer::GenerateActionsIndex(
    const std::string& actions_dir) {
  std::string content = BuildMdSummaryTable(
      actions_dir, "Device Actions");
  if (content.empty()) return;
  WriteFile(actions_dir + "/index.md", content);
  LOG(INFO) << "ToolIndexer: Generated "
            << actions_dir << "/index.md";
}

void ToolIndexer::GenerateEmbeddedIndex(
    const std::string& embedded_dir) {
  std::string content = BuildMdSummaryTable(
      embedded_dir, "Embedded Tools");
  if (content.empty()) return;
  WriteFile(embedded_dir + "/index.md", content);
  LOG(INFO) << "ToolIndexer: Generated "
            << embedded_dir << "/index.md";
}

void ToolIndexer::GenerateCliIndex(
    const std::string& cli_dir) {
  std::error_code ec;
  if (!fs::is_directory(cli_dir, ec)) return;

  std::ostringstream md;
  md << "# CLI Tools\n\n";

  int count = 0;
  for (const auto& entry :
       fs::directory_iterator(cli_dir, ec)) {
    if (!entry.is_directory()) continue;
    auto dirname = entry.path().filename().string();
    if (dirname[0] == '.') continue;

    // Extract CLI name
    std::string cli_name = dirname;
    auto sep = dirname.find("__");
    if (sep != std::string::npos) {
      cli_name = dirname.substr(sep + 2);
    }

    // Read tool.md for description
    std::string tool_md =
        entry.path().string() + "/tool.md";
    std::string content = ReadFile(tool_md);
    std::string title =
        content.empty() ? cli_name
                        : ExtractTitle(content);
    std::string desc = ExtractFirstParagraph(content);
    if (title.empty()) title = cli_name;

    md << "- **" << title << "**: " << desc << "\n";
    count++;
  }

  if (count == 0) {
    md << "_No CLI tools registered._\n";
  } else {
    md << "\nTotal: " << count << " CLI tools\n";
  }

  WriteFile(cli_dir + "/index.md", md.str());
  LOG(INFO) << "ToolIndexer: Generated "
            << cli_dir << "/index.md";
}

void ToolIndexer::GenerateToolsMd(
    const std::string& tools_dir) {
  std::ostringstream md;
  md << "# TizenClaw Tool Catalog\n\n";

  // Read each index.md and append its content
  // (without the H1 title — we provide section
  // headers ourselves)
  auto append_section =
      [&](const std::string& subdir,
          const std::string& section_title) {
        std::string path =
            tools_dir + "/" + subdir + "/index.md";
        std::string content = ReadFile(path);
        if (content.empty()) return;

        md << "## " << section_title << "\n\n";

        // Strip the H1 line and leading blank lines
        std::istringstream stream(content);
        std::string line;
        bool past_title = false;
        while (std::getline(stream, line)) {
          if (!past_title) {
            if (line.size() > 2 && line[0] == '#' &&
                line[1] == ' ') {
              past_title = true;
            }
            continue;
          }
          md << line << "\n";
        }
        md << "\n";
      };

  append_section("skills", "Skills");
  append_section("custom_skills", "Custom Skills");
  append_section("actions", "Device Actions");
  append_section("embedded", "Embedded Tools");
  append_section("cli", "CLI Tools");

  WriteFile(tools_dir + "/tools.md", md.str());
  LOG(INFO) << "ToolIndexer: Generated "
            << tools_dir << "/tools.md";
}

void ToolIndexer::RegenerateAll(
    const std::string& tools_dir) {
  GenerateSkillsIndex(tools_dir + "/skills");
  GenerateCustomSkillsIndex(
      tools_dir + "/custom_skills");
  GenerateActionsIndex(tools_dir + "/actions");
  GenerateEmbeddedIndex(tools_dir + "/embedded");
  GenerateCliIndex(tools_dir + "/cli");
  GenerateToolsMd(tools_dir);
  LOG(INFO) << "ToolIndexer: Regenerated all "
            << "indexes under " << tools_dir;
}

}  // namespace tizenclaw
