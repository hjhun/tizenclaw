---
description: Workflow Index — Summary of all available agent workflows
---

# TizenClaw Agent Workflows

이 디렉토리에는 에이전트가 참조하는 워크플로우 문서가 포함되어 있습니다.
새 워크플로우 추가 시 반드시 이 목록도 함께 업데이트하세요.

> [!IMPORTANT]
> 워크플로우 문서는 해당 기능이 실제 디바이스에서 빌드·배포·동작 검증이
> 완료된 후에만 작성 또는 수정합니다. 검증되지 않은 기능에 대한 워크플로우
> 문서 작성은 금지합니다.

## Workflow List

| Slash Command | File | Description |
|---|---|---|
| `/AGENTS` | `AGENTS.md` | 메인 개발 워크플로우 (Plan → Develop → Verify → Commit) |
| `/gbs_build` | `gbs_build.md` | Tizen gbs build 실행 및 빌드 결과 확인 |
| `/deploy_to_emulator` | `deploy_to_emulator.md` | RPM을 에뮬레이터/디바이스에 sdb로 배포 |
| `/cli_testing` | `cli_testing.md` | tizenclaw-cli를 통한 기능 테스트 |
| `/gtest_integration` | `gtest_integration.md` | gtest & ctest 유닛 테스트 구성 및 실행 |
| `/crash_debug` | `crash_debug.md` | 크래시 덤프 디버깅 (sdb shell + gdb) |
| `/coding_rules` | `coding_rules.md` | 코딩 규칙 및 스타일 가이드 (Google C++ Style) |
| `/commit_guidelines` | `commit_guidelines.md` | Git 커밋 메시지 규칙 (Conventional Commits) |
