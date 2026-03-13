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
#ifndef SKILL_VERIFIER_HH
#define SKILL_VERIFIER_HH

#include <string>
#include <vector>

namespace tizenclaw {

// Validates skill packages before activation.
// Runs a 3-step pipeline: manifest schema, entry point,
// and sandbox dry-run. Skills that fail are marked as
// disabled (verified=false) in their manifest.
class SkillVerifier {
 public:
  struct VerifyResult {
    bool passed = false;
    std::vector<std::string> errors;
    std::vector<std::string> warnings;
  };

  // Run full verification pipeline on a skill directory.
  // skill_dir must contain a manifest.json.
  static VerifyResult Verify(const std::string& skill_dir);

  // Mark skill as disabled in its manifest
  // (sets "verified": false).
  static void DisableSkill(const std::string& skill_dir);

  // Mark skill as verified in its manifest
  // (sets "verified": true).
  static void EnableSkill(const std::string& skill_dir);

  // Valid runtime values
  static bool IsValidRuntime(const std::string& runtime);

 private:
  // Step 1: Validate manifest.json required fields
  static VerifyResult ValidateManifest(
      const std::string& manifest_path);

  // Step 2: Check entry point existence & permissions
  static VerifyResult ValidateEntryPoint(
      const std::string& skill_dir,
      const std::string& entry_point,
      const std::string& runtime);

  // Step 3: Sandbox dry-run with empty args
  static VerifyResult DryRun(
      const std::string& skill_dir,
      const std::string& entry_point,
      const std::string& runtime);
};

}  // namespace tizenclaw

#endif  // SKILL_VERIFIER_HH
