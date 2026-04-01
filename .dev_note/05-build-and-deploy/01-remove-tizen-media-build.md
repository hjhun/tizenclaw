# Build & Deploy: Remove tizen-media-cli

## Modifications Compile
Executed `./deploy.sh -n` for GBS rebuilding. 
The package compiled efficiently since the unused C++ (`tizen-media-cli`) component and its CMake boundaries were purged. `capi-media` dependencies bypassed on compilation seamlessly.

## Deployment Target
x86_64 Emulator test daemon deployed automatically after repackaging.
