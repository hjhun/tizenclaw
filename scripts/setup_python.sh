#!/bin/bash
# Setup Workspace for TizenClaw Python Dependencies

WORKSPACE="$HOME/samba/github/tizenclaw-python"
CURRENT_DIR="$(pwd)"
DEPS_DIR="$CURRENT_DIR/packaging/python-deps"

echo "Creating Workspace: $WORKSPACE"
mkdir -p "$WORKSPACE"

setup_dependency() {
    local dep_name="$1"
    local spec_file="$2"
    local dep_dir="$WORKSPACE/$dep_name"

    echo "--- Setting up $dep_name ---"
    if [ -d "$dep_dir" ]; then
        echo "Directory $dep_dir already exists. Skipping initialization."
        return
    fi
    
    mkdir -p "$dep_dir/packaging"
    
    # Copy the template spec file from the main repository
    if [ -f "$DEPS_DIR/$spec_file" ]; then
        cp "$DEPS_DIR/$spec_file" "$dep_dir/packaging/"
        echo "Copied $spec_file to $dep_dir/packaging/"
    else
        echo "Warning: Spec file $spec_file not found in $DEPS_DIR. Please copy manually."
    fi

    # Initialize a git repository required for gbs build
    cd "$dep_dir" || return
    git init
    git add packaging/
    git commit -m "Initialize $dep_name packaging structure"
    
    echo "Done with $dep_name packaging repository."
    echo ""
}

# 1. Setup python3-protobuf repository
setup_dependency "protobuf" "python3-protobuf.spec"

# 2. Setup python3-onnxruntime repository
setup_dependency "onnxruntime" "python3-onnxruntime.spec"

echo "Workspace is ready at $WORKSPACE."
echo "To build RPMs, download the respective source tarballs into the workspace directories, run 'git add', and execute 'gbs build -A armv7l'."
