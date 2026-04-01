# 05-sandbox-removal.md (Design)

## Deletion Targets
- `packaging/tizenclaw-code-sandbox-debug.service`
- `packaging/tizenclaw-code-sandbox.service`
- `packaging/tizenclaw-code-sandbox.socket`

## Target Overrides
- `CMakeLists.txt`: Lines 56-57
- `packaging/tizenclaw.spec`: Lines 70, 89, 90, 93
- `deploy.sh`: Lines 658, 665-666, 671, 690
