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

#include "package_info_controller.hh"

#include <package_manager.h>

#include <cstdlib>
#include <string>

namespace tizenclaw {
namespace cli {

namespace {

std::string PkgStr(package_info_h info,
                   int (*fn)(package_info_h, char**)) {
  char* v = nullptr;
  if (fn(info, &v) == 0 && v) {
    std::string result(v);
    free(v);
    return result;
  }
  return "";
}

std::string PkgBool(package_info_h info,
                    int (*fn)(package_info_h, bool*)) {
  bool v = false;
  if (fn(info, &v) == 0)
    return v ? "true" : "false";
  return "null";
}

}  // namespace

std::string PackageInfoController::GetInfo(
    const std::string& pkg_id) const {
  package_info_h info = nullptr;
  if (package_info_create(pkg_id.c_str(), &info) != 0)
    return "{\"error\": \"Package not found\"}";

  package_info_installed_storage_type_e st;
  package_info_get_installed_storage(info, &st);
  const char* storage =
      (st == PACKAGE_INFO_INTERNAL_STORAGE)
          ? "internal"
          : "external";

  std::string r =
      "{\"package_id\": \"" + pkg_id + "\", "
      "\"label\": \"" +
      PkgStr(info, package_info_get_label) + "\", "
      "\"version\": \"" +
      PkgStr(info, package_info_get_version) + "\", "
      "\"type\": \"" +
      PkgStr(info, package_info_get_type) + "\", "
      "\"installed_storage\": \"" +
      std::string(storage) + "\", "
      "\"is_system\": " +
      PkgBool(info, package_info_is_system_package) +
      ", \"is_removable\": " +
      PkgBool(info,
              package_info_is_removable_package) +
      ", \"is_preload\": " +
      PkgBool(info,
              package_info_is_preload_package) +
      "}";

  package_info_destroy(info);
  return r;
}

}  // namespace cli
}  // namespace tizenclaw
