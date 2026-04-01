# Design: CLI Test Script
Since no Rust changes are occurring, and the architecture of `tizenclaw-cli` is already locked, the design simply outlines the CLI queries:
- Query `tizen-device-info-cli --battery` to verify correct JSON format.
- Query `tizen-network-info-cli --status` for network interface sanity.
- Query `tizen-app-manager-cli --list` to ensure app context loads successfully.
There are no new FFI boundaries or thread constraints to design.
