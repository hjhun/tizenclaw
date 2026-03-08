cmake_minimum_required(VERSION 3.5)
project(tizenclaw CXX)

# Tizen native compiler flags
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -fPIC")

# Get package information
set(PACKAGE "tizenclaw")
set(VERSION "1.0.0")
set(PREFIX "/usr")
set(BINDIR "${PREFIX}/bin")
set(APP_EXEC_DIR "/usr/bin")
set(APP_RES_DIR "/usr/share/org.tizen.tizenclaw/res")
set(APP_DATA_DIR "/opt/usr/share/tizenclaw")

# Tizen pkg-config dependencies
find_package(PkgConfig REQUIRED)
pkg_check_modules(PKGS REQUIRED
    tizen-core
    glib-2.0
    dlog
    libcurl
    sqlite3
    capi-appfw-tizen-action
)
pkg_check_modules(LIBSOUP REQUIRED libsoup-2.4)
pkg_check_modules(LIBWEBSOCKETS REQUIRED libwebsockets)

# Optional Tizen STT/TTS (not always available)
pkg_check_modules(TIZEN_STT capi-uix-stt)
pkg_check_modules(TIZEN_TTS capi-uix-tts)



# Include directories (plus nlohmann_json)
include_directories(
    src/tizenclaw
    src/tizenclaw/core
    src/tizenclaw/llm
    src/tizenclaw/channel
    src/tizenclaw/storage
    src/tizenclaw/infra
    src/tizenclaw/scheduler
    ${PKGS_INCLUDE_DIRS}
    ${LIBSOUP_INCLUDE_DIRS}
    ${LIBWEBSOCKETS_INCLUDE_DIRS}
    "src/common/nlohmann"
)

if(TIZEN_STT_FOUND)
  include_directories(${TIZEN_STT_INCLUDE_DIRS})
  add_definitions(-DTIZEN_STT_ENABLED)
endif()
if(TIZEN_TTS_FOUND)
  include_directories(${TIZEN_TTS_INCLUDE_DIRS})
  add_definitions(-DTIZEN_TTS_ENABLED)
endif()
add_definitions(-DTIZEN_ACTION_ENABLED)

add_definitions("-DAPP_DATA_DIR=\"${APP_DATA_DIR}\"")

# Source files
set(SOURCES
    src/common/logging.cc
    src/common/file_log_backend.cc
    src/tizenclaw/core/tizenclaw.cc
    src/tizenclaw/core/agent_core.cc
    src/tizenclaw/core/agent_role.cc
    src/tizenclaw/core/pipeline_executor.cc
    src/tizenclaw/core/tool_policy.cc
    src/tizenclaw/core/skill_watcher.cc
    src/tizenclaw/llm/llm_backend_factory.cc
    src/tizenclaw/llm/gemini_backend.cc
    src/tizenclaw/llm/openai_backend.cc
    src/tizenclaw/llm/anthropic_backend.cc
    src/tizenclaw/llm/ollama_backend.cc
    src/tizenclaw/channel/channel_registry.cc
    src/tizenclaw/channel/telegram_client.cc
    src/tizenclaw/channel/mcp_server.cc
    src/tizenclaw/channel/webhook_channel.cc
    src/tizenclaw/channel/slack_channel.cc
    src/tizenclaw/channel/discord_channel.cc
    src/tizenclaw/channel/web_dashboard.cc
    src/tizenclaw/channel/a2a_handler.cc
    src/tizenclaw/channel/voice_channel.cc
    src/tizenclaw/storage/session_store.cc
    src/tizenclaw/storage/embedding_store.cc
    src/tizenclaw/storage/audit_logger.cc
    src/tizenclaw/infra/http_client.cc
    src/tizenclaw/infra/key_store.cc
    src/tizenclaw/infra/container_engine.cc
    src/tizenclaw/infra/health_monitor.cc
    src/tizenclaw/infra/ota_updater.cc
    src/tizenclaw/scheduler/task_scheduler.cc
    src/tizenclaw/core/action_bridge.cc
)


# Executable
add_executable(${PACKAGE} ${SOURCES})
target_link_libraries(${PACKAGE}
    ${PKGS_LIBRARIES}
    ${LIBSOUP_LIBRARIES}
    ${LIBWEBSOCKETS_LIBRARIES}
)
if(TIZEN_STT_FOUND)
  target_link_libraries(${PACKAGE}
      ${TIZEN_STT_LIBRARIES})
endif()
if(TIZEN_TTS_FOUND)
  target_link_libraries(${PACKAGE}
      ${TIZEN_TTS_LIBRARIES})
endif()


# CLI test tool (no Tizen deps — pure POSIX sockets)
add_executable(tizenclaw-cli src/tools/tizenclaw_cli.cc)

# Installation rules
install(TARGETS ${PACKAGE} DESTINATION ${APP_EXEC_DIR})
install(TARGETS tizenclaw-cli DESTINATION ${APP_EXEC_DIR})
install(FILES
    packaging/tizenclaw.service
    packaging/tizenclaw-skills-secure.service
    DESTINATION /usr/lib/systemd/system/
)
install(PROGRAMS
    scripts/run_standard_container.sh
    scripts/skills_secure_container.sh
    DESTINATION /usr/libexec/tizenclaw/
)
install(FILES data/rootfs.tar.gz DESTINATION /opt/usr/share/tizenclaw/)
install(FILES data/llm_config.json.sample data/telegram_config.json.sample data/webhook_config.json.sample data/slack_config.json.sample data/discord_config.json.sample data/agent_roles.json.sample data/a2a_config.json.sample data/ota_config.json.sample data/system_prompt.txt data/tool_policy.json DESTINATION /opt/usr/share/tizenclaw/config/)
install(DIRECTORY skills/ DESTINATION /opt/usr/share/tizenclaw/skills/)
install(DIRECTORY tools/embedded/ DESTINATION /opt/usr/share/tizenclaw/tools/embedded/)
install(DIRECTORY data/web/ DESTINATION /opt/usr/share/tizenclaw/web/)
install(PROGRAMS scripts/start_mcp_tunnel.sh DESTINATION /usr/bin/)
# Unit Tests (run during %check)
enable_testing()
add_subdirectory(test/unit_tests)
