#!/bin/bash
# TizenClaw: Build Alpine Linux RootFS with Debian glibc Python
#
# Usage:
#   ./build_rootfs.sh              # Build for the current host arch
#   TARGET_ARCH=aarch64 ./build_rootfs.sh   # Cross-build for aarch64
#   TARGET_ARCH=armv7l  ./build_rootfs.sh   # Cross-build for armv7l
#
# Creates a minimal Alpine rootfs with Node.js and bash from Alpine
# packages, plus glibc-linked Python 3.11 extracted from Debian
# bookworm .deb packages.  glibc Python is required so that Tizen
# CAPI shared libraries can be loaded via ctypes (musl Python's
# dlopen ignores RTLD_GLOBAL, breaking glibc symbol resolution).

set -e

MOUNTED=false

cleanup() {
    if [ "$MOUNTED" = true ] && [ -n "$ROOTFS_DIR" ]; then
        echo "Cleaning up mounts..."
        sudo umount "$ROOTFS_DIR/proc" 2>/dev/null || true
        sudo umount "$ROOTFS_DIR/sys" 2>/dev/null || true
        sudo umount "$ROOTFS_DIR/dev" 2>/dev/null || true
        MOUNTED=false
    fi
}

trap cleanup EXIT

ALPINE_VERSION="3.20.3"

# Determine target architecture
ARCH="${TARGET_ARCH:-$(uname -m)}"

# Map arch → Alpine download arch + Debian arch
case "$ARCH" in
    x86_64)
        ALPINE_ARCH="x86_64"
        DEB_ARCH="amd64"
        ;;
    aarch64)
        ALPINE_ARCH="aarch64"
        DEB_ARCH="arm64"
        ;;
    armv7l|armv7hl)
        ALPINE_ARCH="armv7"
        DEB_ARCH="armhf"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

TARBALL_URL="https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/${ALPINE_ARCH}/alpine-minirootfs-${ALPINE_VERSION}-${ALPINE_ARCH}.tar.gz"

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_DIR="${SCRIPT_DIR}/.."
DATA_DIR="${PROJECT_DIR}/data"
ROOTFS_DIR="${DATA_DIR}/rootfs_temp"
OUTPUT_TAR="${DATA_DIR}/img/${ARCH}/rootfs.tar.gz"
mkdir -p "${DATA_DIR}/img/${ARCH}"

HOST_ARCH=$(uname -m)
CROSS_BUILD=false
if [ "$ARCH" != "$HOST_ARCH" ]; then
    CROSS_BUILD=true
    echo "Cross-building rootfs for ${ARCH} on ${HOST_ARCH} host"
fi

echo "Using Project Directory: $PROJECT_DIR"

# Clean up any existing temp dir
if [ -d "$ROOTFS_DIR" ]; then
    sudo umount "$ROOTFS_DIR/proc" 2>/dev/null || true
    sudo umount "$ROOTFS_DIR/sys" 2>/dev/null || true
    sudo umount "$ROOTFS_DIR/dev" 2>/dev/null || true
    sudo rm -rf "$ROOTFS_DIR"
fi
mkdir -p "$ROOTFS_DIR"

echo "Downloading Alpine minirootfs for ${ARCH} (${ALPINE_ARCH})..."
wget -qO "$DATA_DIR/alpine.tar.gz" "$TARBALL_URL"

echo "Extracting minirootfs..."
sudo tar -xf "$DATA_DIR/alpine.tar.gz" -C "$ROOTFS_DIR"
rm "$DATA_DIR/alpine.tar.gz"

QEMU_BIN=""
if [ "$CROSS_BUILD" = true ]; then
    case "$ARCH" in
        aarch64)       QEMU_BIN="qemu-aarch64-static" ;;
        armv7l|armv7hl) QEMU_BIN="qemu-arm-static" ;;
    esac
    QEMU_PATH=$(which "$QEMU_BIN" 2>/dev/null || true)
    if [ -z "$QEMU_PATH" ]; then
        echo "Error: $QEMU_BIN not found. Install qemu-user-static." >&2
        exit 1
    fi
    echo "Cross-build: using $QEMU_BIN for chroot..."
    sudo cp "$QEMU_PATH" "$ROOTFS_DIR/usr/bin/"
fi

echo "Copying DNS resolution file..."
sudo cp /etc/resolv.conf "$ROOTFS_DIR/etc/"

echo "Mounting necessary host filesystems for chroot..."
sudo mount -t proc proc "$ROOTFS_DIR/proc/"
sudo mount -o bind /sys "$ROOTFS_DIR/sys/"
sudo mount -o bind /dev "$ROOTFS_DIR/dev/"
MOUNTED=true

# Install Alpine packages (NO python3 — we use Debian's glibc Python)
# and extract Debian glibc Python packages.
echo "Installing Alpine packages and Debian glibc Python..."
cat << EOF | sudo tee "$ROOTFS_DIR/install.sh" > /dev/null
#!/bin/sh
set -e
DEB_ARCH="${DEB_ARCH}"
DEB_MIRROR="http://deb.debian.org/debian"

# 1. Alpine base packages (no python3)
apk update
apk add --no-cache nodejs npm curl ca-certificates bash dpkg

# 2. Download Debian bookworm Packages index to find Python .deb URLs
echo "Resolving Debian Python packages for \${DEB_ARCH}..."
PKGIDX="\${DEB_MIRROR}/dists/bookworm/main/binary-\${DEB_ARCH}/Packages.gz"
curl -sL "\${PKGIDX}" | gzip -d > /tmp/Packages

# Parse package index for .deb filenames
find_deb_path() {
    awk -v pkg="\$1" '
        /^Package: / { name=\$2 }
        /^Filename: / { if (name == pkg) { print \$2; exit } }
    ' /tmp/Packages
}

PY_DEB_PATH=\$(find_deb_path "python3.11-minimal")
LIBPY_DEB_PATH=\$(find_deb_path "libpython3.11-minimal")
STDLIB_DEB_PATH=\$(find_deb_path "libpython3.11-stdlib")

if [ -z "\$PY_DEB_PATH" ] || [ -z "\$LIBPY_DEB_PATH" ] || [ -z "\$STDLIB_DEB_PATH" ]; then
    echo "ERROR: Could not find Debian Python packages" >&2
    exit 1
fi

echo "  python3.11-minimal: \${PY_DEB_PATH}"
echo "  libpython3.11-minimal: \${LIBPY_DEB_PATH}"
echo "  libpython3.11-stdlib: \${STDLIB_DEB_PATH}"

# 3. Download and extract .deb packages
curl -sL -o /tmp/python3.deb "\${DEB_MIRROR}/\${PY_DEB_PATH}"
curl -sL -o /tmp/libpython3.deb "\${DEB_MIRROR}/\${LIBPY_DEB_PATH}"
curl -sL -o /tmp/stdlib.deb "\${DEB_MIRROR}/\${STDLIB_DEB_PATH}"

dpkg -x /tmp/python3.deb /
dpkg -x /tmp/libpython3.deb /
dpkg -x /tmp/stdlib.deb /

# 4. Create python3 symlink
ln -sf python3.11 /usr/bin/python3

# 5. Create glibc dynamic linker symlinks.
# Debian Python's ELF interpreter points to /lib64/ld-linux-x86-64.so.2
# (amd64) or /lib/ld-linux-armhf.so.3 (armhf) or
# /lib/ld-linux-aarch64.so.1 (arm64).  At runtime, host /lib is
# bind-mounted at /host_lib.  Without these symlinks the kernel
# cannot find the interpreter when chroot executes python3.11.
case "\${DEB_ARCH}" in
    amd64)
        mkdir -p /lib64
        ln -sf /host_lib/ld-linux-x86-64.so.2 /lib64/ld-linux-x86-64.so.2
        ;;
    armhf)
        ln -sf /host_lib/ld-linux-armhf.so.3 /lib/ld-linux-armhf.so.3
        ;;
    arm64)
        ln -sf /host_lib/ld-linux-aarch64.so.1 /lib/ld-linux-aarch64.so.1
        ;;
esac

# 6. Clean up
rm -f /tmp/python3.deb /tmp/libpython3.deb /tmp/stdlib.deb /tmp/Packages
apk del dpkg
rm -rf /var/cache/apk/*
EOF
sudo chmod +x "$ROOTFS_DIR/install.sh"
sudo chroot "$ROOTFS_DIR" /install.sh
sudo rm "$ROOTFS_DIR/install.sh"

echo "Unmounting filesystems..."
cleanup

# Clean up QEMU binary from rootfs (not needed at runtime)
if [ -n "$QEMU_BIN" ]; then
    sudo rm -f "$ROOTFS_DIR/usr/bin/$QEMU_BIN"
fi

echo "Creating final rootfs tarball..."
cd "$ROOTFS_DIR"
sudo tar -czf "$OUTPUT_TAR" *
cd "$PROJECT_DIR"

sudo rm -rf "$ROOTFS_DIR"

echo "Success! RootFS created at: $OUTPUT_TAR"
