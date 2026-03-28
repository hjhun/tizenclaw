---
name: workflow_manager
description: "Actively enforces and provides checklists for the TizenClaw development workflow."
category: Developer Tools
risk_level: low
runtime: python
entry_point: workflow_manager.py
---

# Workflow Manager Skill

This skill allows the Agent (and Git Hooks) to explicitly verify that the TizenClaw development rules are being followed before allowing commits to proceed.

It performs the following verifications:
1. **Zero Warnings Policy**: Checks for Rust compiler warnings (`cargo check`).
2. **Deploy Verification**: Verifies `deploy.sh` was run successfully by checking the `.deploy_success` marker file.
3. **Crash Logging**: Verifies there are no recent FATAL app crashes via `sdb shell journalctl` or `dlogutil`.

```json:parameters
{
  "type": "object",
  "properties": {
    "action": {
      "type": "string",
      "description": "The action to perform: 'verify_status' (check build/deploy rules), or 'get_checklist' (get code review requirements).",
      "enum": ["verify_status", "get_checklist"]
    },
    "workflow": {
      "type": "string",
      "description": "If action is 'get_checklist', provide the name of the workflow (e.g. 'code_review')."
    }
  },
  "required": ["action"]
}
```
