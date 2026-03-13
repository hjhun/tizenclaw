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

#include "skill_verifier.hh"

#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <json.hpp>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

constexpr int kDryRunTimeoutSec = 5;

// Fork+exec a command with a timeout, capturing stdout+stderr.
std::pair<std::string, int> RunWithTimeout(
    const std::string& cmd, int timeout_sec) {
  int pipefd[2];
  if (pipe(pipefd) == -1) return {"", -1};

  pid_t pid = fork();
  if (pid == -1) {
    close(pipefd[0]);
    close(pipefd[1]);
    return {"", -1};
  }

  if (pid == 0) {
    close(pipefd[0]);
    dup2(pipefd[1], STDOUT_FILENO);
    dup2(pipefd[1], STDERR_FILENO);
    close(pipefd[1]);
    execl("/usr/bin/bash", "bash", "-c", cmd.c_str(), nullptr);
    _exit(127);
  }

  close(pipefd[1]);

  // Set timeout via alarm-like approach using select
  std::string output;
  char buf[256];
  ssize_t n;

  // Set non-blocking read with timeout
  fd_set fds;
  struct timeval tv;
  tv.tv_sec = timeout_sec;
  tv.tv_usec = 0;

  while (true) {
    FD_ZERO(&fds);
    FD_SET(pipefd[0], &fds);
    int sel = select(pipefd[0] + 1, &fds, nullptr, nullptr, &tv);
    if (sel <= 0) break;
    n = read(pipefd[0], buf, sizeof(buf) - 1);
    if (n <= 0) break;
    buf[n] = '\0';
    output += buf;
    if (output.size() > 4096) break;  // Limit output
  }
  close(pipefd[0]);

  int status = 0;
  pid_t w = waitpid(pid, &status, WNOHANG);
  if (w == 0) {
    // Process still running after timeout — kill it
    kill(pid, SIGKILL);
    waitpid(pid, &status, 0);
    return {output, -2};  // -2 = timeout
  }

  int rc = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return {output, rc};
}

std::string GetEntryPointForRuntime(
    const std::string& name,
    const std::string& runtime) {
  if (runtime == "python") return name + ".py";
  if (runtime == "node") return name + ".js";
  if (runtime == "native") return name;
  return name + ".py";  // default
}

}  // namespace

bool SkillVerifier::IsValidRuntime(const std::string& runtime) {
  return runtime == "python" || runtime == "node" ||
         runtime == "native";
}

SkillVerifier::VerifyResult SkillVerifier::Verify(
    const std::string& skill_dir) {
  namespace fs = std::filesystem;
  VerifyResult final_result;
  final_result.passed = true;

  std::string manifest_path = skill_dir + "/manifest.json";

  // Step 1: Validate manifest
  auto manifest_result = ValidateManifest(manifest_path);
  if (!manifest_result.passed) {
    final_result.passed = false;
    final_result.errors.insert(
        final_result.errors.end(),
        manifest_result.errors.begin(),
        manifest_result.errors.end());
    return final_result;
  }
  final_result.warnings.insert(
      final_result.warnings.end(),
      manifest_result.warnings.begin(),
      manifest_result.warnings.end());

  // Read manifest for runtime and entry_point
  std::string runtime = "python";
  std::string entry_point;
  std::string name;
  try {
    std::ifstream f(manifest_path);
    nlohmann::json j;
    f >> j;
    name = j.value("name", "");
    runtime = j.value("runtime", "python");
    entry_point = j.value("entry_point", "");
    if (entry_point.empty()) {
      entry_point = GetEntryPointForRuntime(name, runtime);
    }
  } catch (...) {
    final_result.passed = false;
    final_result.errors.push_back(
        "Failed to parse manifest.json");
    return final_result;
  }

  // Step 2: Validate entry point
  auto ep_result = ValidateEntryPoint(
      skill_dir, entry_point, runtime);
  if (!ep_result.passed) {
    final_result.passed = false;
    final_result.errors.insert(
        final_result.errors.end(),
        ep_result.errors.begin(),
        ep_result.errors.end());
    return final_result;
  }
  final_result.warnings.insert(
      final_result.warnings.end(),
      ep_result.warnings.begin(),
      ep_result.warnings.end());

  // Step 3: Dry-run
  auto dry_result = DryRun(
      skill_dir, entry_point, runtime);
  if (!dry_result.passed) {
    final_result.passed = false;
    final_result.errors.insert(
        final_result.errors.end(),
        dry_result.errors.begin(),
        dry_result.errors.end());
    return final_result;
  }
  final_result.warnings.insert(
      final_result.warnings.end(),
      dry_result.warnings.begin(),
      dry_result.warnings.end());

  LOG(INFO) << "Skill verified: " << skill_dir;
  return final_result;
}

SkillVerifier::VerifyResult SkillVerifier::ValidateManifest(
    const std::string& manifest_path) {
  VerifyResult result;
  result.passed = true;

  if (!std::filesystem::exists(manifest_path)) {
    result.passed = false;
    result.errors.push_back(
        "manifest.json not found: " + manifest_path);
    return result;
  }

  try {
    std::ifstream f(manifest_path);
    nlohmann::json j;
    f >> j;

    // Required: name
    if (!j.contains("name") ||
        !j["name"].is_string() ||
        j["name"].get<std::string>().empty()) {
      result.passed = false;
      result.errors.push_back(
          "Missing or empty 'name' field");
    }

    // Required: description
    if (!j.contains("description") ||
        !j["description"].is_string()) {
      result.warnings.push_back(
          "Missing 'description' field");
    }

    // Required: parameters
    if (!j.contains("parameters") ||
        !j["parameters"].is_object()) {
      result.passed = false;
      result.errors.push_back(
          "Missing or invalid 'parameters' field");
    }

    // Optional: runtime (validate if present)
    if (j.contains("runtime")) {
      std::string rt = j["runtime"].get<std::string>();
      if (!IsValidRuntime(rt)) {
        result.passed = false;
        result.errors.push_back(
            "Invalid runtime: '" + rt +
            "'. Must be python, node, or native");
      }
    }

    // Optional: language (only meaningful for native)
    if (j.contains("language")) {
      std::string lang = j["language"].get<std::string>();
      std::string rt = j.value("runtime", "python");
      if (rt != "native") {
        result.warnings.push_back(
            "'language' field is only meaningful "
            "for native runtime");
      }
    }

  } catch (const std::exception& e) {
    result.passed = false;
    result.errors.push_back(
        std::string("JSON parse error: ") + e.what());
  }

  return result;
}

SkillVerifier::VerifyResult SkillVerifier::ValidateEntryPoint(
    const std::string& skill_dir,
    const std::string& entry_point,
    const std::string& runtime) {
  VerifyResult result;
  result.passed = true;

  std::string path = skill_dir + "/" + entry_point;

  if (!std::filesystem::exists(path)) {
    result.passed = false;
    result.errors.push_back(
        "Entry point not found: " + path);
    return result;
  }

  // For native runtime, check execute permission
  if (runtime == "native") {
    struct stat st;
    if (stat(path.c_str(), &st) == 0) {
      if (!(st.st_mode & S_IXUSR)) {
        result.passed = false;
        result.errors.push_back(
            "Native binary lacks execute "
            "permission: " + path);
      }
    }
  }

  return result;
}

SkillVerifier::VerifyResult SkillVerifier::DryRun(
    const std::string& skill_dir,
    const std::string& entry_point,
    const std::string& runtime) {
  VerifyResult result;
  result.passed = true;

  std::string script = skill_dir + "/" + entry_point;
  std::string cmd;

  if (runtime == "python") {
    cmd = "CLAW_ARGS='{}' /usr/bin/python3 '" +
          script + "'";
  } else if (runtime == "node") {
    cmd = "CLAW_ARGS='{}' /usr/bin/node '" +
          script + "'";
  } else if (runtime == "native") {
    cmd = "CLAW_ARGS='{}' '" + script + "'";
  } else {
    result.passed = false;
    result.errors.push_back(
        "Cannot dry-run unknown runtime: " + runtime);
    return result;
  }

  auto [output, rc] = RunWithTimeout(cmd, kDryRunTimeoutSec);

  if (rc == -2) {
    result.passed = false;
    result.errors.push_back(
        "Dry-run timed out after " +
        std::to_string(kDryRunTimeoutSec) + "s");
    return result;
  }

  if (rc != 0) {
    // Check if output contains valid JSON error
    // (some skills return {"status":"error",...})
    try {
      auto j = nlohmann::json::parse(output);
      if (j.contains("status") &&
          j["status"] == "error") {
        // Acceptable — skill runs but reports error
        // with empty args. This is expected behavior.
        result.warnings.push_back(
            "Dry-run returned error status "
            "(expected with empty args)");
        return result;
      }
    } catch (...) {
      // Not valid JSON — real crash
    }

    std::string detail = output.substr(
        0, std::min((size_t)200, output.size()));
    result.passed = false;
    result.errors.push_back(
        "Dry-run failed (exit " +
        std::to_string(rc) + "): " + detail);
    return result;
  }

  // Exit 0 — check that output looks like JSON
  std::string trimmed = output;
  while (!trimmed.empty() &&
         (trimmed.back() == '\n' ||
          trimmed.back() == '\r')) {
    trimmed.pop_back();
  }

  if (!trimmed.empty()) {
    // Find last line
    auto pos = trimmed.rfind('\n');
    std::string last_line = (pos != std::string::npos)
                                ? trimmed.substr(pos + 1)
                                : trimmed;
    if (!last_line.empty() &&
        last_line[0] != '{' && last_line[0] != '[') {
      result.warnings.push_back(
          "Dry-run output is not JSON");
    }
  }

  return result;
}

void SkillVerifier::DisableSkill(
    const std::string& skill_dir) {
  std::string path = skill_dir + "/manifest.json";
  try {
    std::ifstream fin(path);
    nlohmann::json j;
    fin >> j;
    fin.close();

    j["verified"] = false;

    std::ofstream fout(path);
    fout << j.dump(4) << std::endl;
    LOG(INFO) << "Skill disabled: " << skill_dir;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to disable skill: "
               << e.what();
  }
}

void SkillVerifier::EnableSkill(
    const std::string& skill_dir) {
  std::string path = skill_dir + "/manifest.json";
  try {
    std::ifstream fin(path);
    nlohmann::json j;
    fin >> j;
    fin.close();

    j["verified"] = true;

    std::ofstream fout(path);
    fout << j.dump(4) << std::endl;
    LOG(INFO) << "Skill enabled: " << skill_dir;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to enable skill: "
               << e.what();
  }
}

}  // namespace tizenclaw
