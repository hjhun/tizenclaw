#!/bin/bash
# TizenClaw: Build Debian minimal RootFS tarball with Python & Node.js
#
# Usage:
#   ./build_rootfs.sh              # Build for the current host arch
#   TARGET_ARCH=aarch64 ./build_rootfs.sh   # Cross-build for aarch64
#   TARGET_ARCH=armv7l  ./build_rootfs.sh   # Cross-build for armv7l
#
# Uses debootstrap to create a minimal Debian (bookworm) rootfs with
# glibc, Python 3, Node.js, and bash.  A pure glibc rootfs avoids
# the musl / glibc ABI incompatibility issues that occur when loading
# Tizen CAPI libraries via ctypes.

set -e

MOUNTED=false

cleanup() {
    if [ "$MOUNTED" = true ] && [ -n "$ROOTFS_DIR" ]; then
        echo "Cleaning up mounts..."
        sudo umount "$ROOTFS_DIR/proc" 2>/dev/null || true
        sudo umount "$ROOTFS_DIR/sys" 2>/dev/null || true
        sudo umount "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
        sudo umount "$ROOTFS_DIR/dev" 2>/dev/null || true
        MOUNTED=false
    fi
}

trap cleanup EXIT

DEBIAN_SUITE="bookworm"
DEBIAN_MIRROR="http://deb.debian.org/debian"

# Determine target architecture (allow cross-build via TARGET_ARCH env)
ARCH="${TARGET_ARCH:-$(uname -m)}"

# Map system arch -> Debian arch + QEMU binary
case "$ARCH" in
    x86_64)
        DEB_ARCH="amd64"
        QEMU_BIN=""
        ;;
    aarch64)
        DEB_ARCH="arm64"
        QEMU_BIN="qemu-aarch64-static"
        ;;
    armv7l|armv7hl)
        DEB_ARCH="armhf"
        QEMU_BIN="qemu-arm-static"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

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
    echo "Cross-building rootfs for ${ARCH} (${DEB_ARCH}) on ${HOST_ARCH} host"
fi

echo "Using Project Directory: $PROJECT_DIR"

# Ensure debootstrap is installed
if ! command -v debootstrap &>/dev/null; then
    echo "Error: debootstrap not found. Install with: sudo apt install debootstrap" >&2
    exit 1
fi

# Clean up any existing temp dir
if [ -d "$ROOTFS_DIR" ]; then
    sudo umount "$ROOTFS_DIR/proc" 2>/dev/null || true
    sudo umount "$ROOTFS_DIR/sys" 2>/dev/null || true
    sudo umount "$ROOTFS_DIR/dev/pts" 2>/dev/null || true
    sudo umount "$ROOTFS_DIR/dev" 2>/dev/null || true
    sudo rm -rf "$ROOTFS_DIR"
fi
mkdir -p "$ROOTFS_DIR"

# Set up QEMU for cross-build
QEMU_PATH=""
if [ "$CROSS_BUILD" = true ] && [ -n "$QEMU_BIN" ]; then
    QEMU_PATH=$(which "$QEMU_BIN" 2>/dev/null || true)
    if [ -z "$QEMU_PATH" ]; then
        echo "Error: $QEMU_BIN not found. Install qemu-user-static." >&2
        exit 1
    fi
    echo "Cross-build: using $QEMU_BIN for debootstrap..."
fi

# Run debootstrap (minimal variant, only essential packages)
echo "Running debootstrap for ${DEB_ARCH} (${DEBIAN_SUITE})..."
DEBOOTSTRAP_OPTS="--variant=minbase --arch=${DEB_ARCH}"
if [ -n "$QEMU_PATH" ]; then
    DEBOOTSTRAP_OPTS="$DEBOOTSTRAP_OPTS --foreign"
fi
sudo debootstrap $DEBOOTSTRAP_OPTS "$DEBIAN_SUITE" "$ROOTFS_DIR" "$DEBIAN_MIRROR"

# Complete second stage for cross-build
if [ "$CROSS_BUILD" = true ] && [ -n "$QEMU_PATH" ]; then
    sudo cp "$QEMU_PATH" "$ROOTFS_DIR/usr/bin/"
    sudo chroot "$ROOTFS_DIR" /debootstrap/debootstrap --second-stage
fi

echo "Mounting host filesystems for chroot..."
sudo mount -t proc proc "$ROOTFS_DIR/proc/"
sudo mount -o bind /sys "$ROOTFS_DIR/sys/"
sudo mount -o bind /dev "$ROOTFS_DIR/dev/"
sudo mount -o bind /dev/pts "$ROOTFS_DIR/dev/pts/" 2>/dev/null || true
MOUNTED=true

echo "Copying DNS resolution file..."
sudo cp /etc/resolv.conf "$ROOTFS_DIR/etc/"

echo "Installing Python 3 and bash..."
cat << 'INSTALL_EOF' | sudo tee "$ROOTFS_DIR/install.sh" > /dev/null
#!/bin/sh
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y --no-install-recommends \
    python3-minimal libpython3-stdlib \
    bash ca-certificates
# Clean up apt cache to minimise image size
apt-get clean
rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*
# Make sure python3 -> python3.11 symlink exists
ls -la /usr/bin/python3*
INSTALL_EOF
sudo chmod +x "$ROOTFS_DIR/install.sh"
sudo chroot "$ROOTFS_DIR" /install.sh
sudo rm "$ROOTFS_DIR/install.sh"

echo "Unmounting filesystems..."
cleanup

# Clean up QEMU binary from rootfs (not needed at runtime)
if [ -n "$QEMU_BIN" ]; then
    sudo rm -f "$ROOTFS_DIR/usr/bin/$QEMU_BIN"
fi

# Clean up debootstrap artifacts
sudo rm -rf "$ROOTFS_DIR/debootstrap"

# Remove docs, man pages, locales to reduce size
sudo rm -rf "$ROOTFS_DIR/usr/share/doc" \
            "$ROOTFS_DIR/usr/share/man" \
            "$ROOTFS_DIR/usr/share/info" \
            "$ROOTFS_DIR/usr/share/locale" \
            "$ROOTFS_DIR/var/log/"* \
            "$ROOTFS_DIR/var/cache/"*

echo "Creating final rootfs tarball..."
cd "$ROOTFS_DIR"
sudo tar -czf "$OUTPUT_TAR" *
cd "$PROJECT_DIR"

sudo rm -rf "$ROOTFS_DIR"

SIZE=$(du -h "$OUTPUT_TAR" | cut -f1)
echo "Success! RootFS created at: $OUTPUT_TAR ($SIZE)"
