CMAKE_MINIMUM_REQUIRED(VERSION 3.12)

PROJECT(tizenclaw CXX)

## Compiler flags
SET(CMAKE_CXX_STANDARD 20)
SET(CMAKE_CXX_STANDARD_REQUIRED ON)
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -fPIC")

# Add IPO/LTO Support
cmake_policy(SET CMP0069 NEW)
include(CheckIPOSupported)
check_ipo_supported(RESULT ipo_supported OUTPUT ipo_output)
if(ipo_supported)
    set(CMAKE_INTERPROCEDURAL_OPTIMIZATION TRUE)
else()
    message(WARNING "IPO is not supported: ${ipo_output}")
endif()

SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-psabi")
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-stringop-overread")
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-stringop-overflow")
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-unused-parameter")
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-unused-result")

## Paths
SET(PREFIX ${CMAKE_INSTALL_PREFIX})
SET(BINDIR "${CMAKE_INSTALL_PREFIX}/bin")
SET(LIBDIR ${LIB_INSTALL_DIR})
SET(INCLUDEDIR "\${prefix}/include")
SET(APP_DATA_DIR "/opt/usr/share/tizenclaw")

## CMake module path (ApplyPkgConfig)
SET(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH}
  "${CMAKE_CURRENT_SOURCE_DIR}/cmake/Modules/")

## Target names
SET(TARGET_TIZENCLAW "tizenclaw")
SET(TARGET_TIZENCLAW_CLI "tizenclaw-cli")

## Find all needed packages once
INCLUDE(FindPkgConfig)
INCLUDE(ApplyPkgConfig)

PKG_CHECK_MODULES(TIZEN_CORE_DEPS REQUIRED tizen-core)
PKG_CHECK_MODULES(GLIB_DEPS REQUIRED glib-2.0)
PKG_CHECK_MODULES(DLOG_DEPS REQUIRED dlog)
PKG_CHECK_MODULES(LIBCURL_DEPS REQUIRED libcurl)
PKG_CHECK_MODULES(SQLITE3_DEPS REQUIRED sqlite3)
PKG_CHECK_MODULES(TIZEN_ACTION_DEPS REQUIRED capi-appfw-tizen-action)
PKG_CHECK_MODULES(AURUM_DEPS REQUIRED libaurum)
PKG_CHECK_MODULES(LIBSOUP_DEPS REQUIRED libsoup-2.4)
PKG_CHECK_MODULES(LIBWEBSOCKETS_DEPS REQUIRED libwebsockets)

# Pkgmgr dependencies
PKG_CHECK_MODULES(PKGMGR_DEPS REQUIRED pkgmgr)
PKG_CHECK_MODULES(PKGMGR_INFO_DEPS REQUIRED pkgmgr-info)
PKG_CHECK_MODULES(PKGMGR_INSTALLER_DEPS REQUIRED pkgmgr-installer)
PKG_CHECK_MODULES(PKGMGR_PARSER_DEPS REQUIRED pkgmgr-parser)

# Optional Tizen STT/TTS (not always available)
PKG_CHECK_MODULES(TIZEN_STT capi-uix-stt)
PKG_CHECK_MODULES(TIZEN_TTS capi-uix-tts)

# Test dependencies
PKG_CHECK_MODULES(GTEST_DEPS REQUIRED gtest gmock)

## Build targets
ADD_SUBDIRECTORY(src)

## Third-party: crun (extract, patch, configure, build)
ADD_SUBDIRECTORY(third_party/crun)

## CLI tools
ADD_SUBDIRECTORY(tools/cli/aurum-cli)


## Unit Tests
ENABLE_TESTING()
ADD_SUBDIRECTORY(test/unit_tests)

## RAG databases, models, and ONNX Runtime are in the
## separate tizenclaw-rag project (../tizenclaw-rag).

INSTALL(FILES "${CMAKE_CURRENT_SOURCE_DIR}/data/sample/tunnel_config.json.sample"
        DESTINATION "${APP_DATA_DIR}/config"
        RENAME "tunnel_config.json")
