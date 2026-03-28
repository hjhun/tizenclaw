#!/bin/bash
# TizenClaw Git Hooks Setup Script

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOOKS_DIR="${PROJECT_DIR}/.git/hooks"
PRE_COMMIT_HOOK="${HOOKS_DIR}/pre-commit"

if [ ! -d "${PROJECT_DIR}/.git" ]; then
    echo "Error: .git directory not found in ${PROJECT_DIR}."
    exit 1
fi

mkdir -p "$HOOKS_DIR"

cat << 'EOF' > "$PRE_COMMIT_HOOK"
#!/bin/bash
# Ensure standard paths are available (especially cargo)
export PATH=$PATH:/usr/local/cargo/bin:${HOME}/.cargo/bin

echo "🔄 Running TizenClaw Workflow checks (pre-commit hook)..."
python3 .agents/skills/workflow_manager/workflow_manager.py --action verify_status
if [ $? -ne 0 ]; then
    echo ""
    echo "❌ COMMIT BLOCKED: Pre-commit workflow verification failed."
    echo "   You MUST adhere to the Dev Workflow policies."
    exit 1
fi
exit 0
EOF

chmod +x "$PRE_COMMIT_HOOK"
echo "✅ Installed pre-commit git hook successfully at ${PRE_COMMIT_HOOK}"
