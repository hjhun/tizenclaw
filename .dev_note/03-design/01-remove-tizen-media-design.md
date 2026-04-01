# Design: Remove tizen-media-cli

## Modifications
- **CMake & Specs:** Remove `capi-media` dependencies from GBS RPM spec and CLI build config.
- **Documents:** Remove any traces matching `tizen-media-cli` from Markdown references.
- **Runtime:** Dropping this component does not disrupt the main asynchronous Rust architecture or other API gateways. No bridging logic needs update.
