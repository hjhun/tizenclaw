CMAKE_MINIMUM_REQUIRED(VERSION 3.5)

PROJECT(tizenclaw CXX)

## Compiler flags
SET(CMAKE_CXX_STANDARD 20)
SET(CMAKE_CXX_STANDARD_REQUIRED ON)
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -fPIC")

# Suppress GCC ARM false-positive warnings on 32-bit ARM
IF(CMAKE_SYSTEM_PROCESSOR MATCHES "^arm")
  SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wno-psabi -Wno-stringop-overread -Wno-stringop-overflow")
ENDIF()

## Paths
SET(PREFIX ${CMAKE_INSTALL_PREFIX})
SET(BINDIR "${PREFIX}/bin")
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
PKG_CHECK_MODULES(LIBSOUP_DEPS REQUIRED libsoup-2.4)
PKG_CHECK_MODULES(LIBWEBSOCKETS_DEPS REQUIRED libwebsockets)

# Optional Tizen STT/TTS (not always available)
PKG_CHECK_MODULES(TIZEN_STT capi-uix-stt)
PKG_CHECK_MODULES(TIZEN_TTS capi-uix-tts)

# Test dependencies
PKG_CHECK_MODULES(GTEST_DEPS REQUIRED gtest gmock)

## Build targets
ADD_SUBDIRECTORY(src)

## Third-party: crun (extract, patch, configure, build)
ADD_SUBDIRECTORY(third_party/crun)

## Unit Tests
ENABLE_TESTING()
ADD_SUBDIRECTORY(test/unit_tests)

## Install optional RAG Database
INSTALL(FILES "${CMAKE_CURRENT_SOURCE_DIR}/data/rag/tizen_knowledge.db"
        DESTINATION "${APP_DATA_DIR}/rag"
        OPTIONAL)
