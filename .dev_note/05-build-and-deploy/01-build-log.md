# Build & Deploy
- Target Architecture: `x86_64`
- Command Executed: `./deploy.sh -a x86_64`
- Observation: GBS build executed without local `cargo build`. Packaging artifacts created and successfully tested by the GBS engine before deployment. Deploy script executed `sdb push` to target and restarted daemon.
