cmake_minimum_required(VERSION 3.10)

project(tizenclaw NONE)

if(POLICY CMP0177)
  cmake_policy(SET CMP0177 NEW)
endif()

include(GNUInstallDirs)

if(DEFINED CARGO_TARGET_TRIPLE)
  set(CARGO_TARGET "${CARGO_TARGET_TRIPLE}")
elseif(DEFINED ENV{CROSS_COMPILE})
  set(CARGO_TARGET "arm-unknown-linux-gnueabi")
else()
  set(CARGO_TARGET "x86_64-unknown-linux-gnu")
endif()

if(CARGO_TARGET STREQUAL "arm-unknown-linux-gnueabi")
  set(TIZENCLAW_ROOTFS_ARCH "armv7l")
elseif(CARGO_TARGET STREQUAL "aarch64-unknown-linux-gnu")
  set(TIZENCLAW_ROOTFS_ARCH "aarch64")
else()
  set(TIZENCLAW_ROOTFS_ARCH "x86_64")
endif()

set(CARGO_TARGET_DIR "${CMAKE_SOURCE_DIR}/target/${CARGO_TARGET}/release")
set(TIZENCLAW_BIN_DIR "${CMAKE_INSTALL_PREFIX}/usr/bin")
set(TIZENCLAW_PACKAGED_DIR "${CMAKE_INSTALL_PREFIX}/opt/usr/share/tizenclaw")
set(TIZENCLAW_PLUGIN_DIR "${TIZENCLAW_PACKAGED_DIR}/plugins")
set(TIZENCLAW_PACKAGED_ASSET_MANIFEST
    "${CMAKE_CURRENT_BINARY_DIR}/tizenclaw-packaged-assets.manifest")
file(GLOB TIZENCLAW_CONFIG_INSTALL_FILES
  RELATIVE "${CMAKE_SOURCE_DIR}"
  "data/config/*"
)
list(FILTER TIZENCLAW_CONFIG_INSTALL_FILES EXCLUDE REGEX "\\.sample$")
list(TRANSFORM TIZENCLAW_CONFIG_INSTALL_FILES
  PREPEND "${CMAKE_SOURCE_DIR}/"
)
list(SORT TIZENCLAW_CONFIG_INSTALL_FILES)

function(tizenclaw_manifest_append list_var entry)
  set(result "${${list_var}}")
  set(current "${entry}")

  while(NOT "${current}" STREQUAL "")
    list(APPEND result "${current}")
    get_filename_component(parent "${current}" DIRECTORY)
    if("${parent}" STREQUAL "" OR "${parent}" STREQUAL "." OR
       "${parent}" STREQUAL "${current}")
      break()
    endif()
    set(current "${parent}")
  endwhile()

  set(${list_var} "${result}" PARENT_SCOPE)
endfunction()

function(tizenclaw_manifest_append_tree list_var install_prefix source_dir)
  set(result "${${list_var}}")

  if(EXISTS "${source_dir}")
    file(GLOB_RECURSE tree_entries RELATIVE "${source_dir}" "${source_dir}/*")
    foreach(tree_entry IN LISTS tree_entries)
      if(NOT IS_DIRECTORY "${source_dir}/${tree_entry}")
        tizenclaw_manifest_append(result "${install_prefix}/${tree_entry}")
      endif()
    endforeach()
  endif()

  set(${list_var} "${result}" PARENT_SCOPE)
endfunction()

set(TIZENCLAW_PACKAGED_ASSET_ENTRIES "")
foreach(config_install_file IN LISTS TIZENCLAW_CONFIG_INSTALL_FILES)
  get_filename_component(config_install_name "${config_install_file}" NAME)
  tizenclaw_manifest_append(
    TIZENCLAW_PACKAGED_ASSET_ENTRIES
    "config/${config_install_name}"
  )
endforeach()
tizenclaw_manifest_append_tree(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "docs"
  "${CMAKE_SOURCE_DIR}/data/docs"
)
tizenclaw_manifest_append_tree(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "embedded"
  "${CMAKE_SOURCE_DIR}/tools/embedded"
)
tizenclaw_manifest_append_tree(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "web"
  "${CMAKE_SOURCE_DIR}/data/web"
)
tizenclaw_manifest_append(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "img/rootfs.tar.gz"
)
tizenclaw_manifest_append(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "plugins/libtizenclaw_plugin.so"
)
tizenclaw_manifest_append(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  "web/img/tizenclaw.svg"
)
tizenclaw_manifest_append(
  TIZENCLAW_PACKAGED_ASSET_ENTRIES
  ".packaged-assets.manifest"
)
list(REMOVE_DUPLICATES TIZENCLAW_PACKAGED_ASSET_ENTRIES)
list(SORT TIZENCLAW_PACKAGED_ASSET_ENTRIES)

file(WRITE "${TIZENCLAW_PACKAGED_ASSET_MANIFEST}" "")
foreach(packaged_asset_entry IN LISTS TIZENCLAW_PACKAGED_ASSET_ENTRIES)
  file(APPEND "${TIZENCLAW_PACKAGED_ASSET_MANIFEST}"
       "${packaged_asset_entry}\n")
endforeach()

add_custom_target(cargo_build ALL
  COMMAND cargo build --release --workspace --target ${CARGO_TARGET}
  WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
  COMMENT "Building Rust workspace for packaging"
)

install(PROGRAMS
  "${CARGO_TARGET_DIR}/tizenclaw"
  "${CARGO_TARGET_DIR}/tizenclaw-cli"
  "${CARGO_TARGET_DIR}/tizenclaw-tool-executor"
  "${CARGO_TARGET_DIR}/tizenclaw-web-dashboard"
  DESTINATION "${TIZENCLAW_BIN_DIR}"
)

install(PROGRAMS
  "${CARGO_TARGET_DIR}/libtizenclaw_plugin.so"
  DESTINATION "${TIZENCLAW_PLUGIN_DIR}"
)

install(FILES
  ${TIZENCLAW_CONFIG_INSTALL_FILES}
  DESTINATION "${TIZENCLAW_PACKAGED_DIR}/config"
)

install(DIRECTORY
  "${CMAKE_SOURCE_DIR}/data/web"
  "${CMAKE_SOURCE_DIR}/data/docs"
  "${CMAKE_SOURCE_DIR}/tools/embedded"
  DESTINATION "${TIZENCLAW_PACKAGED_DIR}"
)

install(FILES
  "${CMAKE_SOURCE_DIR}/data/img/${TIZENCLAW_ROOTFS_ARCH}/rootfs.tar.gz"
  DESTINATION "${TIZENCLAW_PACKAGED_DIR}/img"
  RENAME "rootfs.tar.gz"
)

install(FILES
  "${CMAKE_SOURCE_DIR}/data/img/tizenclaw.svg"
  DESTINATION "${TIZENCLAW_PACKAGED_DIR}/web/img"
)

install(FILES
  "${TIZENCLAW_PACKAGED_ASSET_MANIFEST}"
  DESTINATION "${TIZENCLAW_PACKAGED_DIR}"
  RENAME ".packaged-assets.manifest"
)

install(FILES
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw.service"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-tool-executor.service"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-tool-executor.socket"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/usr/lib/systemd/system"
)

install(PROGRAMS
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-sanitize-packaged-assets.sh"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/usr/libexec/tizenclaw"
  RENAME "sanitize-packaged-assets.sh"
)
