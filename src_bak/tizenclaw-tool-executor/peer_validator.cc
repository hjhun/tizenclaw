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

#include "peer_validator.hh"

#include <linux/limits.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_TOOL_EXECUTOR"

#include "../common/logging.hh"

namespace tizenclaw {
namespace tool_executor {

PeerValidator::PeerValidator(std::vector<std::string> allowed_callers)
    : allowed_callers_(std::move(allowed_callers)) {}

bool PeerValidator::Validate(int client_fd) const {
  struct ucred cred;
  socklen_t len = sizeof(cred);
  if (getsockopt(client_fd, SOL_SOCKET, SO_PEERCRED, &cred, &len) != 0) {
    LOG(ERROR) << "getsockopt SO_PEERCRED failed: " << strerror(errno);
    return false;
  }

  LOG(DEBUG) << "Peer credentials: pid=" << cred.pid
             << " uid=" << cred.uid << " gid=" << cred.gid;
  std::string exe_link = "/proc/" + std::to_string(cred.pid) + "/exe";
  char exe_path[PATH_MAX] = {};
  ssize_t n = readlink(exe_link.c_str(), exe_path, sizeof(exe_path) - 1);
  if (n <= 0) {
    LOG(ERROR) << "readlink " << exe_link << " failed: " << strerror(errno);
    return false;
  }
  exe_path[n] = '\0';

  LOG(DEBUG) << "Peer exe path: " << exe_path;
  std::string basename = exe_path;
  auto slash = basename.rfind('/');
  if (slash != std::string::npos) basename = basename.substr(slash + 1);
  auto del = basename.find(" (deleted)");
  if (del != std::string::npos &&
      del + std::string(" (deleted)").size() == basename.size()) {
    basename = basename.substr(0, del);
  }
  LOG(DEBUG) << "Peer basename: " << basename;

  for (const auto& allowed : allowed_callers_) {
    if (basename == allowed) {
      LOG(INFO) << "Peer validated: pid=" << cred.pid << " exe=" << exe_path;
      return true;
    }
  }

  LOG(WARNING) << "Peer rejected: pid=" << cred.pid
               << " exe=" << exe_path << " basename=" << basename;
  return false;
}

}  // namespace tool_executor
}  // namespace tizenclaw
