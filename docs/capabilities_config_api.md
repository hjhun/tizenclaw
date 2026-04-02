# Capability: Web Dashboard Config API
- **Goal:** Secure REST API over `tarpc` or similar IPC to fetch and edit `app_data/config/*.json`.
- **Inputs:** Bearer token, JSON content, Configuration file name.
- **Outputs:** JSON Status, Modified JSON config files with `.bak` backups.
- **Resource Impact:** Near zero CPU. File I/O happens in single bursts.
