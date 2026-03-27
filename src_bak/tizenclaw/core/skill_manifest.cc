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
#include "skill_manifest.hh"

#include <filesystem>
#include <fstream>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

std::string Trim(const std::string& s) {
  auto start = s.find_first_not_of(" \t\r\n");
  if (start == std::string::npos) return "";
  auto end = s.find_last_not_of(" \t\r\n");
  return s.substr(start, end - start + 1);
}

// Remove surrounding quotes from a YAML value.
std::string StripQuotes(const std::string& s) {
  if (s.size() >= 2) {
    if ((s.front() == '"' && s.back() == '"') ||
        (s.front() == '\'' && s.back() == '\'')) {
      return s.substr(1, s.size() - 2);
    }
  }
  return s;
}

}  // namespace

bool SkillManifest::HasSkillMd(const std::string& skill_dir) {
  return std::filesystem::exists(
      skill_dir + "/SKILL.md");
}

nlohmann::json SkillManifest::Load(
    const std::string& skill_dir) {
  // Priority 1: SKILL.md (Anthropic standard)
  std::string skill_md = skill_dir + "/SKILL.md";
  if (std::filesystem::exists(skill_md)) {
    auto result = ParseSkillMd(skill_md);
    if (!result.empty() && result.contains("name")) {
      LOG(DEBUG) << "Loaded SKILL.md: " << skill_dir;
      return result;
    }
    LOG(WARNING) << "SKILL.md invalid, trying "
                 << "manifest.json: " << skill_dir;
  }

  // Priority 2: manifest.json (legacy)
  std::string manifest = skill_dir + "/manifest.json";
  if (std::filesystem::exists(manifest)) {
    auto result = ParseManifestJson(manifest);
    if (!result.empty()) {
      LOG(DEBUG) << "Loaded manifest.json: " << skill_dir;
      return result;
    }
  }

  return nlohmann::json::object();
}

nlohmann::json SkillManifest::ParseManifestJson(
    const std::string& manifest_path) {
  std::ifstream f(manifest_path);
  if (!f.is_open()) return nlohmann::json::object();

  try {
    nlohmann::json j;
    f >> j;
    return j;
  } catch (const std::exception& e) {
    LOG(WARNING) << "Failed to parse manifest.json: "
                 << manifest_path << ": " << e.what();
    return nlohmann::json::object();
  }
}

nlohmann::json SkillManifest::ParseFrontmatter(
    const std::string& yaml_block) {
  nlohmann::json result;
  std::istringstream iss(yaml_block);
  std::string line;

  while (std::getline(iss, line)) {
    line = Trim(line);
    if (line.empty()) continue;

    auto colon_pos = line.find(':');
    if (colon_pos == std::string::npos) continue;

    std::string key = Trim(line.substr(0, colon_pos));
    std::string value =
        Trim(line.substr(colon_pos + 1));

    if (key.empty()) continue;

    // Strip surrounding quotes from values
    value = StripQuotes(value);

    if (!value.empty())
      result[key] = value;
  }

  return result;
}

nlohmann::json SkillManifest::ExtractParametersBlock(
    const std::string& body) {
  // Look for ```json:parameters ... ``` block
  const std::string marker = "```json:parameters";
  auto start = body.find(marker);
  if (start == std::string::npos) {
    // Also try ```json\n as fallback for parameters
    // block tagged just as json
    return nlohmann::json::object();
  }

  auto block_start = body.find('\n', start);
  if (block_start == std::string::npos)
    return nlohmann::json::object();
  block_start++;

  auto block_end = body.find("```", block_start);
  if (block_end == std::string::npos)
    return nlohmann::json::object();

  std::string json_str =
      Trim(body.substr(block_start,
                       block_end - block_start));

  try {
    return nlohmann::json::parse(json_str);
  } catch (const std::exception& e) {
    LOG(WARNING) << "Failed to parse parameters "
                 << "block: " << e.what();
    return nlohmann::json::object();
  }
}

nlohmann::json SkillManifest::ParseSkillMd(
    const std::string& skill_md_path) {
  std::ifstream f(skill_md_path);
  if (!f.is_open()) return nlohmann::json::object();

  std::string content(
      (std::istreambuf_iterator<char>(f)),
      std::istreambuf_iterator<char>());
  f.close();

  if (content.empty()) return nlohmann::json::object();

  // Extract YAML frontmatter between --- markers
  std::string trimmed = Trim(content);
  if (!trimmed.starts_with("---")) {
    LOG(WARNING) << "SKILL.md missing frontmatter: "
                 << skill_md_path;
    return nlohmann::json::object();
  }

  auto first_delim = trimmed.find("---");
  auto second_delim =
      trimmed.find("---", first_delim + 3);
  if (second_delim == std::string::npos) {
    LOG(WARNING) << "SKILL.md unclosed frontmatter: "
                 << skill_md_path;
    return nlohmann::json::object();
  }

  std::string yaml_content =
      trimmed.substr(first_delim + 3,
                     second_delim - first_delim - 3);
  std::string body =
      trimmed.substr(second_delim + 3);

  // Parse frontmatter key-value pairs
  nlohmann::json result = ParseFrontmatter(yaml_content);

  // Extract parameters from Markdown code block
  nlohmann::json params =
      ExtractParametersBlock(body);
  if (!params.empty()) {
    result["parameters"] = params;
  } else if (!result.contains("parameters")) {
    // Default empty parameters
    result["parameters"] = {
        {"type", "object"},
        {"properties", nlohmann::json::object()},
        {"required", nlohmann::json::array()}};
  }

  return result;
}

std::string SkillManifest::GenerateSkillMd(
    const nlohmann::json& manifest) {
  std::ostringstream oss;

  // YAML frontmatter
  oss << "---\n";
  if (manifest.contains("name"))
    oss << "name: " << manifest["name"].get<std::string>()
        << "\n";
  if (manifest.contains("description"))
    oss << "description: \""
        << manifest["description"].get<std::string>()
        << "\"\n";
  if (manifest.contains("category"))
    oss << "category: "
        << manifest["category"].get<std::string>()
        << "\n";
  if (manifest.contains("risk_level"))
    oss << "risk_level: "
        << manifest["risk_level"].get<std::string>()
        << "\n";
  if (manifest.contains("runtime"))
    oss << "runtime: "
        << manifest["runtime"].get<std::string>()
        << "\n";
  if (manifest.contains("entry_point"))
    oss << "entry_point: "
        << manifest["entry_point"].get<std::string>()
        << "\n";
  if (manifest.contains("language"))
    oss << "language: "
        << manifest["language"].get<std::string>()
        << "\n";
  if (manifest.contains("verified"))
    oss << "verified: "
        << (manifest["verified"].get<bool>()
                ? "true"
                : "false")
        << "\n";
  oss << "---\n\n";

  // Markdown heading
  std::string name = manifest.value("name", "Skill");
  oss << "# " << name << "\n\n";

  // Description
  if (manifest.contains("description")) {
    oss << manifest["description"].get<std::string>()
        << "\n\n";
  }

  // Parameters block
  if (manifest.contains("parameters")) {
    oss << "```json:parameters\n";
    oss << manifest["parameters"].dump(4) << "\n";
    oss << "```\n";
  }

  return oss.str();
}

}  // namespace tizenclaw
