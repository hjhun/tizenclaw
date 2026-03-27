if(EXISTS "/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests")
  if(NOT EXISTS "/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests[1]_tests.cmake" OR
     NOT "/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests[1]_tests.cmake" IS_NEWER_THAN "/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests" OR
     NOT "/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests[1]_tests.cmake" IS_NEWER_THAN "${CMAKE_CURRENT_LIST_FILE}")
    include("/usr/share/cmake/Modules/GoogleTestAddTests.cmake")
    gtest_discover_tests_impl(
      TEST_EXECUTABLE [==[/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests]==]
      TEST_EXECUTOR [==[]==]
      TEST_WORKING_DIR [==[/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit]==]
      TEST_EXTRA_ARGS [==[]==]
      TEST_PROPERTIES [==[]==]
      TEST_PREFIX [==[]==]
      TEST_SUFFIX [==[]==]
      TEST_FILTER [==[]==]
      NO_PRETTY_TYPES [==[FALSE]==]
      NO_PRETTY_VALUES [==[FALSE]==]
      TEST_LIST [==[tizenclaw-unittests_TESTS]==]
      CTEST_FILE [==[/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests[1]_tests.cmake]==]
      TEST_DISCOVERY_TIMEOUT [==[5]==]
      TEST_DISCOVERY_EXTRA_ARGS [==[]==]
      TEST_XML_OUTPUT_DIR [==[]==]
    )
  endif()
  include("/home/abuild/rpmbuild/BUILD/tizenclaw-1.0.0/tests/unit/tizenclaw-unittests[1]_tests.cmake")
else()
  add_test(tizenclaw-unittests_NOT_BUILT tizenclaw-unittests_NOT_BUILT)
endif()
