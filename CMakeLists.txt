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
set(APP_EXEC_DIR "/usr/apps/org.tizen.tizenclaw/bin")
set(APP_RES_DIR "/usr/apps/org.tizen.tizenclaw/res")
set(APP_DATA_DIR "/usr/apps/org.tizen.tizenclaw/data")

# Tizen pkg-config dependencies
find_package(PkgConfig REQUIRED)
pkg_check_modules(PKGS REQUIRED
    capi-appfw-service-application
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
    src/tizenclaw.cpp
    src/agent_core.cpp
    src/container_engine.cpp
)

# Executable
add_executable(${PACKAGE} ${SOURCES})
target_link_libraries(${PACKAGE} ${PKGS_LIBRARIES})

# Installation rules
install(TARGETS ${PACKAGE} DESTINATION ${APP_EXEC_DIR})
install(FILES tizen-manifest.xml DESTINATION /usr/apps/org.tizen.tizenclaw/)
install(FILES data/rootfs.tar.gz DESTINATION /usr/apps/org.tizen.tizenclaw/data/)
install(DIRECTORY skills/ DESTINATION /usr/apps/org.tizen.tizenclaw/data/skills/)

# Unit Tests (run during %check)
enable_testing()
add_subdirectory(test/unit_tests)
