---
description: Tizen gtest & ctest Unit Testing Workflow
---

# TizenClaw Unit Testing Workflow

This module is modeled after the gtest/gmock testing structure of Tizen `rpc-port`, and is designed to run automatically via `ctest` in the `%check` section of the RPM packaging during GBS builds.

## 1. Directory Structure and File Naming
Collect the unit test codes in the `test/unit_tests/` directory at the root of the project.
- `main.cc`: Initializes the gtest environment (`testing::InitGoogleTest`) and runs all tests (`RUN_ALL_TESTS()`)
- `CMakeLists.txt`: Generates the test executable file and sets up `gtest` and `gmock` linking
- `mock/`: Where gmock classes and fake implementation parts for system APIs or other modules are gathered
- `*_test.cc`: Unit test files for each component or class (e.g., `agent_core_test.cc`)

## 2. CMakeLists.txt Configuration Rules
- In `test/unit_tests/CMakeLists.txt`, you must either re-link the `tizenclaw` target implementation files (shared/static library integration) or compile them along with the `*_test.cc` files in `add_executable()` to generate a single unified binary (e.g., `tizenclaw-unittests`).
- Bring in `gtest` and `gmock` dependencies using `pkg_check_modules` and link them (`target_link_libraries`).
- For the Tizen platform, add `add_test(NAME TizenClawTests COMMAND tizenclaw-unittests)` so that CTest recognizes it.

## 3. RPM Spec File (`%check` section)
The `packaging/tizenclaw.spec` file must include the following.
```spec
%check
cd build
ctest -V
```
This section is executed immediately after compiling (`%build`) during the `gbs build` process. If `ctest` fails, the entire package build will also fail.

## 4. Mock Creation and DLOG Hooking
- Handle Tizen CAPIs (like `dlog_print`) with empty shell macros in `main.cc` or `mock/` headers, or redefine them to redirect to `printf`, so they do not fail or interfere in external environments (e.g., gbs build chroot).
- For complex internal components (such as the LXC Container engine), use `gmock` to control dependencies and test behavior-oriented scenarios.

## Development Guide
When writing new code:
1. Create a `_test.cc` file for the corresponding component in `test/unit_tests/`.
2. Write scenarios using the `TEST_F` or `TEST` macros.
3. Add the source to CMakeLists and confirm that the `%check` passes normally when running in the local environment or via `gbs build`.
