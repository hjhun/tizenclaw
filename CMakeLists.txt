cmake_minimum_required(VERSION 3.5)
project(tizenclaw CXX)

# Tizen native compiler flags
set(CMAKE_CXX_STANDARD 17)
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
)
pkg_check_modules(LIBSOUP REQUIRED libsoup-2.4)
pkg_check_modules(LIBWEBSOCKETS REQUIRED libwebsockets)


# Include directories (plus nlohmann_json)
include_directories(
    src/tizenclaw
    ${PKGS_INCLUDE_DIRS}
    ${LIBSOUP_INCLUDE_DIRS}
    ${LIBWEBSOCKETS_INCLUDE_DIRS}
    "src/common/nlohmann"
)

add_definitions("-DAPP_DATA_DIR=\"${APP_DATA_DIR}\"")

# Source files
set(SOURCES
    src/common/logging.cc
    src/common/file_log_backend.cc
    src/tizenclaw/tizenclaw.cc
    src/tizenclaw/agent_core.cc
    src/tizenclaw/container_engine.cc
    src/tizenclaw/http_client.cc
    src/tizenclaw/llm_backend_factory.cc
    src/tizenclaw/gemini_backend.cc
    src/tizenclaw/openai_backend.cc
    src/tizenclaw/anthropic_backend.cc
    src/tizenclaw/ollama_backend.cc
    src/tizenclaw/telegram_client.cc
    src/tizenclaw/session_store.cc
    src/tizenclaw/mcp_server.cc
    src/tizenclaw/tool_policy.cc
    src/tizenclaw/key_store.cc
    src/tizenclaw/audit_logger.cc
    src/tizenclaw/task_scheduler.cc
    src/tizenclaw/channel_registry.cc
    src/tizenclaw/skill_watcher.cc
    src/tizenclaw/webhook_channel.cc
    src/tizenclaw/slack_channel.cc
    src/tizenclaw/discord_channel.cc
)

# Executable
add_executable(${PACKAGE} ${SOURCES})
target_link_libraries(${PACKAGE}
    ${PKGS_LIBRARIES}
    ${LIBSOUP_LIBRARIES}
    ${LIBWEBSOCKETS_LIBRARIES}
)

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
install(FILES data/llm_config.json.sample data/telegram_config.json.sample data/webhook_config.json.sample data/slack_config.json.sample data/discord_config.json.sample data/system_prompt.txt data/tool_policy.json DESTINATION /opt/usr/share/tizenclaw/config/)
install(DIRECTORY skills/ DESTINATION /opt/usr/share/tizenclaw/skills/)
install(PROGRAMS scripts/start_mcp_tunnel.sh DESTINATION /usr/bin/)
# Unit Tests (run during %check)
enable_testing()
add_subdirectory(test/unit_tests)
