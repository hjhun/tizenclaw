/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include <dlog.h>
#include <glib.h>
#include <pkgmgr-info.h>
#include <pkgmgr_installer_info.h>
#include <pkgmgr_parser.h>

#include <cstring>
#include <string>

#undef PROJECT_TAG
#define PROJECT_TAG "TIZENCLAW_METADATA_PLUGIN"

#include "../../common/logging.hh"

#undef EXPORT
#define EXPORT __attribute__((visibility("default")))

namespace {

constexpr const char kMetadataTizenclawLlmBackend[] =
    "http://tizen.org/metadata/tizenclaw/llm-backend";

bool HasPlatformPrivilege() {
  pkgmgr_privilege_level level = PM_PRIVILEGE_UNKNOWN;
  int ret = pkgmgr_installer_info_get_privilege_level(&level);
  // In Tizen, the success return code is usually 0
  if (ret != 0) {
    LOG(ERROR) << "Failed to get privilege level";
    return false;
  }

  return (level == PM_PRIVILEGE_PLATFORM);
}

}  // namespace

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_INSTALL(const char* pkgid,
                                                     const char* appid,
                                                     GList* metadata) {
  LOG(INFO) << "package=" << pkgid;
  GList* iter = metadata;
  while (iter != nullptr) {
    __metadata_t* md = static_cast<__metadata_t*>(iter->data);
    if (!strcmp(md->key, kMetadataTizenclawLlmBackend)) {
      if (!HasPlatformPrivilege()) {
        LOG(ERROR) << "Package(" << pkgid
                   << ") was not signed by platform level certificate";
        return -1;  // Reject installation
      }
      LOG(INFO) << "Package(" << pkgid
                << ") has valid platform signature for TizenClaw LLM backend";
      break;
    }
    iter = g_list_next(iter);
  }

  return 0;  // Allow installation
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_UPGRADE(const char* pkgid,
                                                     const char* appid,
                                                     GList* metadata) {
  LOG(INFO) << "package=" << pkgid;
  GList* iter = metadata;
  while (iter != nullptr) {
    __metadata_t* md = static_cast<__metadata_t*>(iter->data);
    if (!strcmp(md->key, kMetadataTizenclawLlmBackend)) {
      if (!HasPlatformPrivilege()) {
        LOG(ERROR) << "Package(" << pkgid
                   << ") was not signed by platform level certificate";
        return -1;  // Reject upgrade
      }
      LOG(INFO) << "Package(" << pkgid
                << ") has valid platform signature for TizenClaw LLM backend";
      break;
    }
    iter = g_list_next(iter);
  }

  return 0;
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_UNINSTALL(const char* pkgid,
                                                       const char* appid,
                                                       GList* metadata) {
  return 0;
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_CLEAN(const char* pkgid,
                                                   const char* appid,
                                                   GList* metadata) {
  return 0;
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_UNDO(const char* pkgid,
                                                  const char* appid,
                                                  GList* metadata) {
  return 0;
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_REMOVED(const char* pkgid,
                                                     const char* appid,
                                                     GList* metadata) {
  return 0;
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_RECOVERINSTALL(const char* pkgid,
                                                            const char* appid,
                                                            GList* metadata) {
  return PKGMGR_MDPARSER_PLUGIN_INSTALL(pkgid, appid, metadata);
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_RECOVERUPGRADE(const char* pkgid,
                                                            const char* appid,
                                                            GList* metadata) {
  return PKGMGR_MDPARSER_PLUGIN_UPGRADE(pkgid, appid, metadata);
}

extern "C" EXPORT int PKGMGR_MDPARSER_PLUGIN_RECOVERUNINSTALL(const char* pkgid,
                                                              const char* appid,
                                                              GList* metadata) {
  return 0;
}
