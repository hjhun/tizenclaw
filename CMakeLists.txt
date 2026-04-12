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

set(CARGO_TARGET_DIR "${CMAKE_SOURCE_DIR}/target/${CARGO_TARGET}/release")
set(TIZENCLAW_BIN_DIR "${CMAKE_INSTALL_PREFIX}/usr/bin")
set(TIZENCLAW_DATA_DIR "${CMAKE_INSTALL_PREFIX}/opt/usr/share/tizenclaw")
set(TIZENCLAW_PLUGIN_DIR "${TIZENCLAW_DATA_DIR}/plugins")

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
  "${CMAKE_SOURCE_DIR}/data/config/agent_roles.json"
  "${CMAKE_SOURCE_DIR}/data/config/system_prompt.txt"
  DESTINATION "${TIZENCLAW_DATA_DIR}/config"
)

install(DIRECTORY
  "${CMAKE_SOURCE_DIR}/data/web"
  "${CMAKE_SOURCE_DIR}/data/docs"
  "${CMAKE_SOURCE_DIR}/tools/embedded"
  DESTINATION "${TIZENCLAW_DATA_DIR}"
)

install(FILES
  "${CMAKE_SOURCE_DIR}/data/img/tizenclaw.svg"
  DESTINATION "${TIZENCLAW_DATA_DIR}/web/img"
)

install(FILES
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw.service"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/usr/lib/systemd/system"
)
