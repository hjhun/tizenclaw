cmake_minimum_required(VERSION 3.10)
project(tizenclaw-python VERSION 1.0.0 LANGUAGES NONE)

# ── Install python package structure ──
# We install the Python infra into /opt/usr/share/tizenclaw-python
install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/src_py/
        DESTINATION /opt/usr/share/tizenclaw-python
        FILES_MATCHING PATTERN "*.py")

# ── Install executables ──
# We rename them properly so they act as standard binaries in /usr/bin
install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/src_py/tizenclaw_daemon.py 
        DESTINATION bin RENAME tizenclaw)

install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/src_py/tizenclaw_daemon.py 
        DESTINATION bin RENAME tizenclaw-daemon)

install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/src_py/tizenclaw_cli.py 
        DESTINATION bin RENAME tizenclaw-cli)

install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/src_py/tizenclaw_tool_executor.py 
        DESTINATION bin RENAME tizenclaw-tool-executor)

install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/src_py/tizenclaw_code_sandbox.py 
        DESTINATION bin RENAME tizenclaw-code-sandbox)

# ── Install systemd service and scripts ──
install(FILES 
        ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw.service
        ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw-tool-executor.service
        ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw-tool-executor.socket
        ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw-code-sandbox.service
        ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw-code-sandbox.socket
        DESTINATION /usr/lib/systemd/system)

install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/packaging/tizenclaw.manifest 
        DESTINATION /opt/usr/share/tizenclaw-python)

if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/scripts/run_standard_container.sh)
    install(PROGRAMS ${CMAKE_CURRENT_SOURCE_DIR}/scripts/run_standard_container.sh 
            DESTINATION /opt/usr/share/tizenclaw/scripts)
endif()

# ── Install web dashboard ──
if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/data/web)
    install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/data/web/
            DESTINATION /opt/usr/share/tizenclaw/web)
endif()

# ── Install dev config files ──
if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/data/devel)
    install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/data/devel/
            DESTINATION /opt/usr/share/tizenclaw/data/devel
            FILES_MATCHING PATTERN "*.json")
endif()
