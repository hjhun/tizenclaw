# Local Build Cleanup Rule

Direct `cargo check`, `cargo build`, `cargo test`, or ad-hoc `cmake`
execution is **prohibited** in this project.
Default development builds/tests must go through `./devel_host.sh`.
Use `./deploy.sh` only when the user explicitly asks for the
Tizen/emulator/device path.

If a local build is accidentally executed, the following cleanup **must** be performed immediately.

## Cleanup Steps

Run the following commands from the workspace root:

```bash
# 1. Remove Cargo/Rust build artifacts
rm -rf target/

# 2. Remove CMake build artifacts (root level)
rm -f CMakeCache.txt Makefile cmake_install.cmake
rm -rf CMakeFiles/ build_local/

# 3. Remove CMake build artifacts scattered in subdirectories
find . -path './vendor' -prune -o \
  \( -name 'Makefile' -o -name 'cmake_install.cmake' \) -type f -delete
find ./src ./test -name 'CMakeFiles' -type d -exec rm -rf {} + 2>/dev/null
find ./src ./test \( -name '*.o' -o -name '*.d' -o -name 'link.d' \) -delete 2>/dev/null

# 4. Remove generated shared libraries and object files (root level)
find . -maxdepth 1 \( -name '*.o' -o -name '*.so' -o -name '*.so.*' \) -delete 2>/dev/null
```

## What is Already Covered by `.gitignore`

The following patterns are in `.gitignore` to prevent accidental commits:

| Pattern | Description |
|---------|-------------|
| `/target/` | Cargo build output |
| `/build/` | Top-level build directory |
| `build_local/` | Local build directory |
| `CMakeCache.txt` | CMake cache |
| `CMakeFiles/` | CMake generated files (all subdirs) |
| `Makefile` | CMake generated makefile |
| `cmake_install.cmake` | CMake install script |
| `*.o`, `*.d` | Object files and dependency files |
| `*.so`, `*.so.*` | Shared library files |

## Prevention

Before running any build command, always verify you are using the right
script for the cycle:

```bash
# Correct: default host development
./devel_host.sh
./devel_host.sh --test

# Correct: explicit Tizen/emulator/device cycle
./deploy.sh
./deploy.sh --test

# WRONG - do NOT run locally
# cargo build
# cargo check
# cmake .
```
