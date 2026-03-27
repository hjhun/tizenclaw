# CLI test tool utilizing the libtizenclaw CAPI
ADD_EXECUTABLE(${TARGET_TIZENCLAW_CLI}
  main.cc
  socket_client.cc
  request_handler.cc
  response_printer.cc
  interactive_shell.cc
  tool_prober.cc
)
TARGET_INCLUDE_DIRECTORIES(${TARGET_TIZENCLAW_CLI} PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}/../common"
)
TARGET_LINK_LIBRARIES(${TARGET_TIZENCLAW_CLI} 
  tizenclaw-api
)
INSTALL(TARGETS ${TARGET_TIZENCLAW_CLI} DESTINATION ${BINDIR})
