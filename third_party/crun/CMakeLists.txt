# Build crun from source tarball via CMake custom commands
# Steps: extract tarball → apply patches → configure → make

SET(CRUN_VERSION "1.26")
SET(CRUN_SRC_TAR "${CMAKE_CURRENT_SOURCE_DIR}/src/crun-${CRUN_VERSION}.tar.gz")
SET(CRUN_BUILD_DIR "${CMAKE_CURRENT_BINARY_DIR}/crun-${CRUN_VERSION}")
SET(CRUN_BINARY "${CRUN_BUILD_DIR}/crun")
SET(CRUN_PATCHES_DIR "${CMAKE_CURRENT_SOURCE_DIR}/patches")

# Collect all patch files
FILE(GLOB CRUN_PATCHES "${CRUN_PATCHES_DIR}/*.patch")

# Create a build script that runs all steps
FILE(WRITE "${CMAKE_CURRENT_BINARY_DIR}/build_crun.sh"
"#!/bin/sh
set -e

CRUN_SRC_TAR=\"${CRUN_SRC_TAR}\"
CRUN_BUILD_DIR=\"${CRUN_BUILD_DIR}\"
CRUN_PATCHES_DIR=\"${CRUN_PATCHES_DIR}\"

# 1. Clean and extract
rm -rf \"$CRUN_BUILD_DIR\"
tar -xf \"$CRUN_SRC_TAR\" -C \"${CMAKE_CURRENT_BINARY_DIR}\"

# 2. Apply patches
for p in \"$CRUN_PATCHES_DIR\"/*.patch; do
  if [ -f \"$p\" ]; then
    echo \"Applying patch: $p\"
    patch -p1 -d \"$CRUN_BUILD_DIR\" < \"$p\"
  fi
done

# 3. Strip -Werror flags for third-party code and configure
CFLAGS=$(echo \"$CFLAGS\" | sed 's/-Werror[^ ]*//g')
export CFLAGS=\"$CFLAGS -Wno-shadow -Wno-unused-variable -Wno-unused-function -Wno-stringop-overread -Wno-stringop-overflow -Wno-sign-compare\"
cd \"$CRUN_BUILD_DIR\"
export YAJL_CFLAGS=\" \"
export YAJL_LIBS=\" \"
./configure \\
    --prefix=/usr \\
    --disable-systemd \\
    --disable-shared \\
    --enable-embedded-yajl \\
    --disable-caps \\
    --disable-seccomp

# 4. Build
make -j$(nproc 2>/dev/null || echo 1)

echo \"crun ${CRUN_VERSION} built successfully.\"
")

ADD_CUSTOM_COMMAND(
  OUTPUT ${CRUN_BINARY}
  COMMAND sh "${CMAKE_CURRENT_BINARY_DIR}/build_crun.sh"
  DEPENDS ${CRUN_SRC_TAR} ${CRUN_PATCHES}
  COMMENT "Building crun ${CRUN_VERSION} from source (with patches)"
)

ADD_CUSTOM_TARGET(crun ALL DEPENDS ${CRUN_BINARY})

INSTALL(PROGRAMS ${CRUN_BINARY} DESTINATION /usr/libexec/tizenclaw/)
