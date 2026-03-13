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
#ifndef TOOL_INDEXER_HH_
#define TOOL_INDEXER_HH_

#include <string>

namespace tizenclaw {

// Generates hierarchical index.md summaries for each
// tool category (skills, custom_skills, actions,
// embedded) and a top-level tools.md that aggregates
// all category indexes.
//
// Directory layout (under tools_dir):
//   tools/
//   ├── tools.md              (aggregated catalog)
//   ├── skills/index.md       (skill manifest table)
//   ├── custom_skills/index.md
//   ├── actions/index.md
//   └── embedded/index.md
class ToolIndexer {
 public:
  // Regenerate all index.md files and tools.md.
  // This is the main entry point — call after
  // any tool-set change (skill reload, action
  // sync, custom skill CRUD).
  static void RegenerateAll(
      const std::string& tools_dir);

  // Generate index.md for skills/ directory
  // by scanning each subdirectory's manifest.json.
  static void GenerateSkillsIndex(
      const std::string& skills_dir);

  // Generate index.md for custom_skills/ directory.
  static void GenerateCustomSkillsIndex(
      const std::string& custom_skills_dir);

  // Generate index.md for actions/ directory
  // by extracting H1 titles and first paragraphs
  // from each .md file.
  static void GenerateActionsIndex(
      const std::string& actions_dir);

  // Generate index.md for embedded/ directory
  // by extracting H1 titles and first paragraphs
  // from each .md file.
  static void GenerateEmbeddedIndex(
      const std::string& embedded_dir);

  // Generate index.md for cli/ directory
  // by extracting H1 titles and first paragraphs
  // from each tool.md file.
  static void GenerateCliIndex(
      const std::string& cli_dir);

  // Generate tools.md by reading all index.md files.
  static void GenerateToolsMd(
      const std::string& tools_dir);
};

}  // namespace tizenclaw

#endif  // TOOL_INDEXER_HH_
