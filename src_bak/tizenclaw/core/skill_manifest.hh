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
#ifndef SKILL_MANIFEST_HH_
#define SKILL_MANIFEST_HH_

#include <json.hpp>
#include <string>

namespace tizenclaw {

// Unified skill manifest loader that supports both:
//   1. SKILL.md (Anthropic standard: YAML frontmatter + Markdown)
//   2. manifest.json (legacy JSON format)
//
// Priority: SKILL.md > manifest.json
//
// SKILL.md format (Anthropic standard):
//   ---
//   name: skill_name
//   description: "What the skill does"
//   ---
//   # Skill Title
//   Documentation body (when to use, parameters, output...)
//
// Only `name` and `description` are in the frontmatter.
// Parameters and other details are documented in the
// Markdown body as human-readable documentation.
class SkillManifest {
 public:
  // Load skill metadata from a skill directory.
  // Tries SKILL.md first, falls back to manifest.json.
  // Returns empty JSON object on failure.
  static nlohmann::json Load(const std::string& skill_dir);

  // Check if a skill directory has a SKILL.md file.
  static bool HasSkillMd(const std::string& skill_dir);

  // Parse a SKILL.md file and return as JSON compatible
  // with the legacy manifest.json schema.
  static nlohmann::json ParseSkillMd(
      const std::string& skill_md_path);

  // Parse legacy manifest.json file.
  static nlohmann::json ParseManifestJson(
      const std::string& manifest_path);

  // Generate a SKILL.md string from a JSON manifest.
  static std::string GenerateSkillMd(
      const nlohmann::json& manifest);

 private:
  // Parse YAML frontmatter from SKILL.md content.
  // Returns key-value map as JSON.
  static nlohmann::json ParseFrontmatter(
      const std::string& yaml_block);

  // Extract JSON parameters block from Markdown body.
  // Looks for ```json:parameters ... ``` block.
  static nlohmann::json ExtractParametersBlock(
      const std::string& body);
};

}  // namespace tizenclaw

#endif  // SKILL_MANIFEST_HH_
