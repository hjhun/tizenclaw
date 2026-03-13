# aurum-cli: Native C++ CLI for Aurum UI Automation
# Supports two backends:
#   - Default: libaurum direct (AT-SPI2)
#   - --grpc:  aurum-bootstrap gRPC server

FIND_PACKAGE(PkgConfig REQUIRED)
PKG_CHECK_MODULES(AURUM_CLI_DEPS REQUIRED
  libaurum
  ecore
  tizen-core
  dlog
  grpc++
  protobuf
)

SET(AURUM_CLI_TARGET "aurum-cli")

# ─── Proto code generation ──────────────────
SET(AURUM_PROTO "${CMAKE_CURRENT_SOURCE_DIR}/aurum.proto")
SET(PROTO_GEN_DIR "${CMAKE_CURRENT_BINARY_DIR}")

FIND_PROGRAM(PROTOC protoc REQUIRED)
FIND_PROGRAM(GRPC_CPP_PLUGIN grpc_cpp_plugin REQUIRED)

# aurum.pb.cc / aurum.pb.h
ADD_CUSTOM_COMMAND(
  OUTPUT
    "${PROTO_GEN_DIR}/aurum.pb.cc"
    "${PROTO_GEN_DIR}/aurum.pb.h"
  COMMAND ${PROTOC}
    --proto_path=${CMAKE_CURRENT_SOURCE_DIR}
    --cpp_out=${PROTO_GEN_DIR}
    ${AURUM_PROTO}
  DEPENDS ${AURUM_PROTO}
  COMMENT "Generating protobuf stubs from aurum.proto"
)

# aurum.grpc.pb.cc / aurum.grpc.pb.h
ADD_CUSTOM_COMMAND(
  OUTPUT
    "${PROTO_GEN_DIR}/aurum.grpc.pb.cc"
    "${PROTO_GEN_DIR}/aurum.grpc.pb.h"
  COMMAND ${PROTOC}
    --proto_path=${CMAKE_CURRENT_SOURCE_DIR}
    --grpc_out=${PROTO_GEN_DIR}
    --plugin=protoc-gen-grpc=${GRPC_CPP_PLUGIN}
    ${AURUM_PROTO}
  DEPENDS ${AURUM_PROTO}
  COMMENT "Generating gRPC stubs from aurum.proto"
)

# ─── Build ──────────────────────────────────
ADD_EXECUTABLE(${AURUM_CLI_TARGET}
  aurum_cli.cc
  aurum_watcher.cc
  aurum_grpc_client.cc
  "${PROTO_GEN_DIR}/aurum.pb.cc"
  "${PROTO_GEN_DIR}/aurum.grpc.pb.cc"
)

TARGET_INCLUDE_DIRECTORIES(${AURUM_CLI_TARGET} PRIVATE
  ${AURUM_CLI_DEPS_INCLUDE_DIRS}
  ${PROTO_GEN_DIR}
)

TARGET_LINK_LIBRARIES(${AURUM_CLI_TARGET}
  ${AURUM_CLI_DEPS_LIBRARIES}
)

TARGET_COMPILE_OPTIONS(${AURUM_CLI_TARGET} PRIVATE
  ${AURUM_CLI_DEPS_CFLAGS_OTHER}
)

# ─── Install ────────────────────────────────
SET(AURUM_CLI_INSTALL_DIR
  "/opt/usr/share/tizenclaw/tools/cli/aurum-cli")

INSTALL(TARGETS ${AURUM_CLI_TARGET}
  DESTINATION ${AURUM_CLI_INSTALL_DIR}
)

INSTALL(FILES
  "${CMAKE_CURRENT_SOURCE_DIR}/tool.md"
  DESTINATION ${AURUM_CLI_INSTALL_DIR}
)
