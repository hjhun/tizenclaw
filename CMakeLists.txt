CMAKE_MINIMUM_REQUIRED(VERSION 3.12)

PROJECT(tizenclaw)
INCLUDE(GNUInstallDirs)

## Paths
SET(PREFIX ${CMAKE_INSTALL_PREFIX})
SET(BINDIR "${CMAKE_INSTALL_PREFIX}/bin")
SET(APP_DATA_DIR "/opt/usr/share/tizenclaw")

## ═══════════════════════════════════════════
##  Cargo/Rust Build (primary build system)
## ═══════════════════════════════════════════
# OpenSSL is statically linked via vendored build (no system OpenSSL needed)
# Strip LTO flags from CFLAGS because rustc's linker doesn't load the GCC LTO plugin automatically,
# which causes undefined references when linking static C libraries built by the cc crate.
string(REGEX REPLACE "-flto[^ ]*" "" SAFE_CFLAGS "$ENV{CFLAGS}")

ADD_CUSTOM_TARGET(cargo_build ALL
  COMMAND env "CFLAGS=${SAFE_CFLAGS} -Wno-error=missing-field-initializers" cargo build --release --offline --manifest-path ${CMAKE_SOURCE_DIR}/Cargo.toml
  WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
  COMMENT "Building Rust workspace with Cargo (LTO disabled for C dependencies)"
)

## ═══════════════════════════════════════════
##  Install Rust binaries
## ═══════════════════════════════════════════
INSTALL(PROGRAMS "${CMAKE_SOURCE_DIR}/target/release/tizenclaw"
        DESTINATION "${BINDIR}")

INSTALL(PROGRAMS "${CMAKE_SOURCE_DIR}/target/release/tizenclaw-cli"
        DESTINATION "${BINDIR}")

INSTALL(PROGRAMS "${CMAKE_SOURCE_DIR}/target/release/tizenclaw-tool-executor"
        DESTINATION "${BINDIR}")

## ═══════════════════════════════════════════
##  Install data & config files
## ═══════════════════════════════════════════

# Config sample
INSTALL(FILES "${CMAKE_SOURCE_DIR}/data/sample/tunnel_config.json.sample"
        DESTINATION "${APP_DATA_DIR}/config"
        RENAME "tunnel_config.json")

# Systemd service/socket units
INSTALL(FILES
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw.service"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-tool-executor.service"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-tool-executor.socket"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-code-sandbox.service"
  "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-code-sandbox.socket"
  DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/systemd/system")

# Web dashboard static files
INSTALL(DIRECTORY "${CMAKE_SOURCE_DIR}/data/web/"
        DESTINATION "${APP_DATA_DIR}/web")

# Tool routing guides
INSTALL(FILES
  "${CMAKE_SOURCE_DIR}/tools/routing_guide.md"
  "${CMAKE_SOURCE_DIR}/tools/tools.md"
  DESTINATION "/opt/usr/share/tizen-tools")

# Embedded tool descriptors
INSTALL(DIRECTORY "${CMAKE_SOURCE_DIR}/tools/embedded/"
        DESTINATION "/opt/usr/share/tizen-tools/embedded")

# CLI tool scripts
INSTALL(DIRECTORY "${CMAKE_SOURCE_DIR}/tools/cli/"
        DESTINATION "/opt/usr/share/tizen-tools/cli"
        PATTERN "CMakeLists.txt" EXCLUDE)

# System CLI tools — create empty dir
INSTALL(DIRECTORY DESTINATION "/opt/usr/share/tizen-tools/system_cli")

# Code executor removed (now pure shell)

# MCP tunnel script
INSTALL(PROGRAMS "${CMAKE_SOURCE_DIR}/scripts/start_mcp_tunnel.sh"
        DESTINATION "${BINDIR}")

# RAG web docs
INSTALL(FILES "${CMAKE_SOURCE_DIR}/data/rag/web.zip"
        DESTINATION "${APP_DATA_DIR}/rag")

## ═══════════════════════════════════════════
##  Install libtizenclaw.so (plugin framework)
## ═══════════════════════════════════════════
INSTALL(FILES "${CMAKE_SOURCE_DIR}/target/release/libtizenclaw.so"
        DESTINATION "${CMAKE_INSTALL_FULL_LIBDIR}")

## ═══════════════════════════════════════════
##  Install libtizenclaw_client.so (C API) & headers
## ═══════════════════════════════════════════
INSTALL(FILES "${CMAKE_SOURCE_DIR}/target/release/libtizenclaw_client.so"
        DESTINATION "${CMAKE_INSTALL_FULL_LIBDIR}")

# C header
INSTALL(FILES "${CMAKE_SOURCE_DIR}/src/libtizenclaw-client/include/tizenclaw.h"
        DESTINATION "${CMAKE_INSTALL_PREFIX}/include/tizenclaw")

# pkgconfig
CONFIGURE_FILE(
    "${CMAKE_SOURCE_DIR}/packaging/tizenclaw.pc.in"
    "${CMAKE_BINARY_DIR}/tizenclaw.pc"
    @ONLY
)
INSTALL(FILES "${CMAKE_BINARY_DIR}/tizenclaw.pc"
        DESTINATION "${CMAKE_INSTALL_FULL_LIBDIR}/pkgconfig")

## ═══════════════════════════════════════════
##  Install libtizenclaw_sdk.so (plugin SDK)
## ═══════════════════════════════════════════
INSTALL(FILES "${CMAKE_SOURCE_DIR}/target/release/libtizenclaw_sdk.so"
        DESTINATION "${CMAKE_INSTALL_FULL_LIBDIR}")

# Plugin SDK headers
INSTALL(FILES
    "${CMAKE_SOURCE_DIR}/src/libtizenclaw-sdk/include/tizenclaw_error.h"
    "${CMAKE_SOURCE_DIR}/src/libtizenclaw-sdk/include/tizenclaw_channel.h"
    "${CMAKE_SOURCE_DIR}/src/libtizenclaw-sdk/include/tizenclaw_llm_backend.h"
    "${CMAKE_SOURCE_DIR}/src/libtizenclaw-sdk/include/tizenclaw_curl.h"
    DESTINATION "${CMAKE_INSTALL_PREFIX}/include/tizenclaw")

# pkgconfig for tizenclaw-sdk
CONFIGURE_FILE(
    "${CMAKE_SOURCE_DIR}/packaging/tizenclaw-core.pc.in"
    "${CMAKE_BINARY_DIR}/tizenclaw-sdk.pc"
    @ONLY
)
INSTALL(FILES "${CMAKE_BINARY_DIR}/tizenclaw-sdk.pc"
        DESTINATION "${CMAKE_INSTALL_FULL_LIBDIR}/pkgconfig")

