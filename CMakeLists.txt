cmake_minimum_required(VERSION 3.0)
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

# Include directories (plus nlohmann_json)
include_directories(
    inc
    ${PKGS_INCLUDE_DIRS}
    "inc/nlohmann"
)

add_definitions("-DAPP_DATA_DIR=\"${APP_DATA_DIR}\"")

# Source files
set(SOURCES
    src/tizenclaw.cc
    src/agent_core.cc
    src/container_engine.cc
    src/http_client.cc
    src/llm_backend_factory.cc
    src/gemini_backend.cc
    src/openai_backend.cc
    src/anthropic_backend.cc
    src/ollama_backend.cc
    src/telegram_bridge.cc
    src/session_store.cc
)

# Executable
add_executable(${PACKAGE} ${SOURCES})
target_link_libraries(${PACKAGE} ${PKGS_LIBRARIES})

# Installation rules
install(TARGETS ${PACKAGE} DESTINATION ${APP_EXEC_DIR})
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
install(FILES data/rootfs.tar.gz data/llm_config.json.sample data/telegram_config.json.sample DESTINATION /opt/usr/share/tizenclaw/)
install(DIRECTORY skills/ DESTINATION /opt/usr/share/tizenclaw/skills/)
install(PROGRAMS scripts/start_mcp_tunnel.sh DESTINATION /usr/bin/)
# Unit Tests (run during %check)
enable_testing()
add_subdirectory(test/unit_tests)
