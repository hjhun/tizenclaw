# Planning: Exhaustive CLI Tool Verification

## Objective
Directly execute ALL 80 available CLI tools on the Tizen emulator, collect raw output, and review each tool's operational status.

## Scope
- 88 total tool directories detected
- 80 have executable binaries (EXEC)
- 8 missing binaries (MISS): tizen-capi-media-vision-cli, tizen-capi-media-vision-dl-cli, tizen-capi-system-system-settings-cli, tizen-capi-ui-autofill-cli, tizen-capi-ui-inputmethod-cli, tizen-capi-ui-inputmethod-manager-cli, tizen-sensor-cli, tizen-webkit2-cli, tizen-webrtc-display-cli, tizen-widget-service-cli

## Execution Mode
- **One-shot Worker**: Each tool is invoked once with `--help` and once with a representative subcommand.
- No code changes required. Build already deployed.
