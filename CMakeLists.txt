CMAKE_MINIMUM_REQUIRED(VERSION 3.12)

PROJECT(tizenclaw CXX)

## Compiler flags
SET(CMAKE_CXX_STANDARD 20)
SET(CMAKE_CXX_STANDARD_REQUIRED ON)
SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -Wshadow -Wunused -fPIC")

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

# Fix transitive libxml2 dependency for libsoup-2.4, libpkgmgr-parser,
# libhal-api-common, etc. LINK_LIBRARIES appends to end of link line.
LINK_LIBRARIES(xml2)

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

# Event adapter dependencies
PKG_CHECK_MODULES(APP_EVENT_DEPS REQUIRED capi-appfw-event)

PKG_CHECK_MODULES(AUL_DEPS REQUIRED aul)
PKG_CHECK_MODULES(RUA_DEPS REQUIRED rua)
PKG_CHECK_MODULES(VCONF_DEPS REQUIRED vconf vconf-internal-keys)

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

## CLI tools — Target names
SET(TARGET_TIZEN_CONTROL_DISPLAY_CLI
  "tizen-control-display-cli")
SET(TARGET_TIZEN_DEVICE_INFO_CLI
  "tizen-device-info-cli")
SET(TARGET_TIZEN_HARDWARE_CONTROL_CLI
  "tizen-hardware-control-cli")
SET(TARGET_TIZEN_SOUND_CLI
  "tizen-sound-cli")
SET(TARGET_TIZEN_NETWORK_INFO_CLI
  "tizen-network-info-cli")
SET(TARGET_TIZEN_APP_MANAGER_CLI
  "tizen-app-manager-cli")
SET(TARGET_TIZEN_NOTIFICATION_CLI
  "tizen-notification-cli")
SET(TARGET_TIZEN_SENSOR_CLI
  "tizen-sensor-cli")
SET(TARGET_TIZEN_WEB_SEARCH_CLI
  "tizen-web-search-cli")
SET(TARGET_TIZEN_FILE_MANAGER_CLI
  "tizen-file-manager-cli")
SET(TARGET_TIZEN_MEDIA_CLI
  "tizen-media-cli")

## CLI tools — Dependencies (declared once)
PKG_CHECK_MODULES(CAPI_SYSTEM_DEVICE_DEPS REQUIRED
  capi-system-device)
PKG_CHECK_MODULES(CAPI_SYSTEM_INFO_DEPS REQUIRED
  capi-system-info)
PKG_CHECK_MODULES(CAPI_SYSTEM_RUNTIME_INFO_DEPS REQUIRED
  capi-system-runtime-info)
PKG_CHECK_MODULES(CAPI_SYSTEM_SETTINGS_DEPS REQUIRED
  capi-system-system-settings)
PKG_CHECK_MODULES(CAPI_SYSTEM_SENSOR_DEPS REQUIRED
  capi-system-sensor)
PKG_CHECK_MODULES(STORAGE_DEPS REQUIRED storage)
PKG_CHECK_MODULES(FEEDBACK_DEPS REQUIRED feedback)
PKG_CHECK_MODULES(CAPI_NETWORK_WIFI_DEPS REQUIRED
  capi-network-wifi-manager)
PKG_CHECK_MODULES(CAPI_NETWORK_CONNECTION_DEPS REQUIRED
  capi-network-connection)
PKG_CHECK_MODULES(CAPI_NETWORK_BT_DEPS REQUIRED
  capi-network-bluetooth)
PKG_CHECK_MODULES(CAPI_APPFW_APP_MANAGER_DEPS REQUIRED
  capi-appfw-app-manager)
PKG_CHECK_MODULES(CAPI_APPFW_APP_CONTROL_DEPS REQUIRED
  capi-appfw-app-control)
PKG_CHECK_MODULES(CAPI_APPFW_ALARM_DEPS REQUIRED
  capi-appfw-alarm)
PKG_CHECK_MODULES(CAPI_APPFW_PACKAGE_MANAGER_DEPS REQUIRED
  capi-appfw-package-manager)
PKG_CHECK_MODULES(CAPI_MEDIA_SOUND_MANAGER_DEPS REQUIRED
  capi-media-sound-manager)
PKG_CHECK_MODULES(CAPI_MEDIA_TONE_PLAYER_DEPS REQUIRED
  capi-media-tone-player)
PKG_CHECK_MODULES(NOTIFICATION_DEPS REQUIRED
  notification)
PKG_CHECK_MODULES(CAPI_CONTENT_MEDIA_CONTENT_DEPS
  capi-content-media-content)
PKG_CHECK_MODULES(CAPI_MEDIA_METADATA_EXTRACTOR_DEPS
  capi-media-metadata-extractor)
PKG_CHECK_MODULES(CAPI_CONTENT_MIME_TYPE_DEPS
  capi-content-mime-type)

IF(CAPI_CONTENT_MEDIA_CONTENT_DEPS_FOUND AND
   CAPI_MEDIA_METADATA_EXTRACTOR_DEPS_FOUND AND
   CAPI_CONTENT_MIME_TYPE_DEPS_FOUND)
  SET(BUILD_TIZEN_MEDIA_CLI TRUE)
  MESSAGE(STATUS "tizen-media-cli: ENABLED")
ELSE()
  SET(BUILD_TIZEN_MEDIA_CLI FALSE)
  MESSAGE(STATUS "tizen-media-cli: DISABLED "
    "(missing media deps)")
ENDIF()

ADD_SUBDIRECTORY(tools)

## Unit Tests
ENABLE_TESTING()
ADD_SUBDIRECTORY(test/unit_tests)

## ONNX Runtime, RAG databases, OCR models, and embedding model
## are provided by the tizenclaw-assets package.

INSTALL(FILES "${CMAKE_CURRENT_SOURCE_DIR}/data/sample/tunnel_config.json.sample"
        DESTINATION "${APP_DATA_DIR}/config"
        RENAME "tunnel_config.json")
