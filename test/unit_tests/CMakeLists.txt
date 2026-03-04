cmake_minimum_required(VERSION 3.0)

# Make sure we build unittests if not a subproject
if(NOT TARGET ${PACKAGE})
    project(tizenclaw-unittests CXX)
    set(CMAKE_CXX_STANDARD 17)
    set(CMAKE_CXX_STANDARD_REQUIRED ON)
endif()

find_package(PkgConfig REQUIRED)
pkg_check_modules(GTEST_DEPS REQUIRED gtest gmock)
pkg_check_modules(APPFW_DEPS REQUIRED capi-appfw-service-application dlog libcurl)

# Test Sources
set(UNIT_TEST_SRCS
    main.cc
    agent_core_test.cc
    container_engine_test.cc
)

# Mock Sources
set(MOCK_SRCS
    # mock/mock_dlog.cc
)

# Main project sources to test (excluding main())
set(CORE_SRCS
    ${CMAKE_CURRENT_SOURCE_DIR}/../../src/agent_core.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/../../src/container_engine.cpp
)

add_executable(tizenclaw-unittests
    ${UNIT_TEST_SRCS}
    ${MOCK_SRCS}
    ${CORE_SRCS}
)

target_include_directories(tizenclaw-unittests PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/mock
    ${CMAKE_CURRENT_SOURCE_DIR}/../../inc
    ${CMAKE_CURRENT_SOURCE_DIR}/../../inc/nlohmann
    ${GTEST_DEPS_INCLUDE_DIRS}
    ${APPFW_DEPS_INCLUDE_DIRS}
)

target_link_libraries(tizenclaw-unittests PUBLIC
    ${GTEST_DEPS_LIBRARIES}
    ${APPFW_DEPS_LIBRARIES}
    pthread
)

# CTest integration
enable_testing()
add_test(NAME TizenClawTests COMMAND tizenclaw-unittests)

# Install test bin (optional, can be installed for runtime)
install(TARGETS tizenclaw-unittests DESTINATION /usr/apps/org.tizen.tizenclaw/bin)
