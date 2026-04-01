# Development Phase: CLI Tool Manifest Fixes

## 1. Issue Identification
A bash test script was executed traversing all 11 Tizen native CLI utilities. The resulting log `/tmp/cli_test_results.txt` exposed `tizenclaw-cli` LLM hallucination in parsing 6 core endpoints:
1. `tizen-device-info-cli` - Failed to understand `battery` subcommand as positional.
2. `tizen-file-manager-cli` - Missed the `--path` required flag.
3. `tizen-media-cli` - Confused valid target configurations.
4. `tizen-network-info-cli` - Failed to utilize explicit positional arguments (like `wifi`).
5. `tizen-sensor-cli` - Failed to map options to the `--type` flag.
6. `tizen-web-search-cli` - Failed to map the query string into the `--query` flag.

## 2. Implemented Fixes
The `tool.md` for each affected CLI was updated. We injected an explicit `## LLM Agent Instructions` block detailing exact formatting examples and strict parameter bindings. No internal `.rs` logic or testing via local `cargo build/test` was required as the failure isolated to JSON/String prompt mapping exclusively.

Ready for Build & Deploy to sweep these markdown updates into the compiled RPM and deploy them to the target Tizen emulator.
