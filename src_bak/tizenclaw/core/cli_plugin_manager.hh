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

#ifndef CLI_PLUGIN_MANAGER_HH
#define CLI_PLUGIN_MANAGER_HH

#include <functional>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <vector>

#include "../infra/pkgmgr_client.hh"

namespace tizenclaw {

// Manages CLI tool injection from TPK packages.
// Listens for pkgmgr events and creates symlinks
// from TPK bin/ and res/ directories into
// /opt/usr/share/tizen-tools/cli/.
//
// Each CLI tool directory contains:
//   - executable  (symlink to the TPK binary)
//   - tool.md     (symlink to the TPK res/ descriptor)
//
// The tool.md file provides rich documentation
// (subcommands, options, examples) that the LLM
// reads to understand how to invoke the CLI tool.
class CliPluginManager : public PkgmgrClient::IListener {
 public:
  static CliPluginManager& GetInstance();

  bool Initialize();
  void Shutdown();

  using ChangeCallback = std::function<void()>;
  void SetChangeCallback(ChangeCallback cb) { change_callback_ = cb; }

  // Get all CLI tool names installed from TPK packages
  std::set<std::string> GetInstalledCliDirs() const;

  // Parse CLI tool names from metadata value (supports | delimiter)
  static std::vector<std::string> ParseCliNames(const std::string& value);

  // Link a CLI tool from TPK to CLI tools dir
  bool LinkCliTool(const std::string& pkg_root,
                   const std::string& cli_name,
                   const std::string& target_dir);

  // Remove a CLI tool directory
  void RemoveCliDir(const std::string& target);

 private:
  CliPluginManager();
  ~CliPluginManager();

  CliPluginManager(const CliPluginManager&) = delete;
  CliPluginManager& operator=(const CliPluginManager&) = delete;

  void OnPkgmgrEvent(std::shared_ptr<PkgmgrEventArgs> args) override;

  void HandleInstallEvent(const std::string& pkgid);
  void HandleUpdateEvent(const std::string& pkgid);
  void HandleUninstallEvent(const std::string& pkgid);

  bool LoadCliFromPkg(const std::string& pkgid);
  void UnloadCliFromPkg(const std::string& pkgid);

  // Collect all metadata values for the CLI key from a package
  static std::vector<std::string> CollectCliMetadata(
      const std::string& pkgid);

  std::mutex map_mutex_;
  std::map<std::string, std::shared_ptr<PkgmgrEventArgs>> package_events_;

  // pkgid -> list of installed CLI tool directory names
  mutable std::mutex cli_mutex_;
  std::map<std::string, std::vector<std::string>> pkg_cli_tools_;

  ChangeCallback change_callback_;

  static constexpr const char* kCliDir =
      "/opt/usr/share/tizen-tools/cli";
  static constexpr const char* kMetadataKey =
      "http://tizen.org/metadata/tizenclaw/cli";
};

}  // namespace tizenclaw

#endif  // CLI_PLUGIN_MANAGER_HH
