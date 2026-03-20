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
#ifndef CONTAINER_ENGINE_HH
#define CONTAINER_ENGINE_HH

#include <memory>
#include <string>

namespace tizenclaw {

class ContainerEngine {
 public:
  ContainerEngine();
  ~ContainerEngine();

  // Initialize the container backend (crun or runc)
  [[nodiscard]] bool Initialize();

  // Execute a skill: tries UDS socket first, then
  // crun exec fallback, then host-direct fallback.
  [[nodiscard]] std::string ExecuteSkill(const std::string& skill_name,
                                         const std::string& arg_str);

  // Execute arbitrary Python code via the skill
  // executor's execute_code command.
  [[nodiscard]] std::string ExecuteCode(const std::string& code);

  // Execute file operations via the skill
  // executor's file_manager command.
  [[nodiscard]] std::string ExecuteFileOp(const std::string& operation,
                                          const std::string& path,
                                          const std::string& content);

  // Execute a system CLI tool via the tool-executor
  // process (prevents running arbitrary binaries in
  // the daemon process).
  [[nodiscard]] std::string ExecuteCliTool(const std::string& tool_name,
                                           const std::string& arguments,
                                           int timeout_seconds);

  [[nodiscard]] std::string StartCliSession(const std::string& tool_name,
                                           const std::string& arguments,
                                           const std::string& mode,
                                           int timeout_seconds);
  [[nodiscard]] std::string SendToCliSession(const std::string& session_id,
                                            const std::string& input,
                                            int read_timeout_ms = 2000);
  [[nodiscard]] std::string ReadCliSession(const std::string& session_id,
                                          int read_timeout_ms = 1000);
  [[nodiscard]] std::string CloseCliSession(const std::string& session_id);

 private:
  // Execute skill via Unix Domain Socket to the
  // skill_executor running in the secure container.
  std::string ExecuteSkillViaSocket(const std::string& skill_name,
                                    const std::string& arg_str);

  // Legacy: exec into running OCI container
  std::string ExecuteSkillViaCrun(const std::string& skill_name,
                                  const std::string& arg_str);

  bool EnsureSkillsContainerRunning();
  bool PrepareSkillsBundle();
  bool PrepareOverlayUsr();
  void CleanupOverlayUsr();
  bool IsContainerRunning() const;
  bool StartSkillsContainer();
  void StopSkillsContainer();
  bool WriteSkillsConfig() const;
  std::string BuildPaths(const std::string& leaf) const;
  std::string EscapeShellArg(const std::string& input) const;
  std::string CrunCmd(const std::string& subcmd) const;
  std::string FindPython3() const;

  // Connect to tool-executor via abstract namespace socket.
  // Returns fd >= 0 on success, -1 on failure.
  int ConnectToToolExecutor() const;

  std::string ExecuteToolExecutorCommand(const nlohmann::json& req,
                                         int timeout_seconds = 30);

  // Extract last JSON-like line from raw output
  static std::string ExtractJsonResult(const std::string& raw);

  bool initialized_;
  std::string runtime_bin_;
  std::string app_data_dir_;
  std::string skills_dir_;
  std::string bundle_dir_;
  std::string rootfs_tar_;
  std::string container_id_;
  std::string crun_root_;

  // Abstract namespace socket name (no leading '\0' — added by connect code)
  static constexpr const char kToolExecutorSocketName[] =
      "tizenclaw-tool-executor.sock";
};

}  // namespace tizenclaw

#endif  // CONTAINER_ENGINE_HH
