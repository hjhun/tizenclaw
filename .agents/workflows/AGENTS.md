---
description: Main Development Workflow (Plan -> Develop -> Verify)
---

# TizenClaw Main Development Workflow

이 워크플로우는 TizenClaw 프로젝트의 핵심 개발 프로세스(계획 -> 개발 -> 검증)를 정의합니다. AGENT는 항상 이 프로세스를 따라 작업을 수행해야 합니다.

## 1. Plan (계획)
- 목적과 요구사항을 정확히 파악합니다.
- 기존 코드 분석 및 적용 가능한 워크플로우(`/coding_rules`, `/commit_guidelines` 등)를 확인합니다.
- 구현 전에 작업 단위(`task.md`)를 작성하고 세부 계획을 세웁니다.

## 2. Develop (개발 & 로컬 검증)
- 소스 코드를 수정하고 단위 테스트를 추가/수정합니다.
- `gbs_build.md` 워크플로우를 참조하여 로컬에서 코드를 빌드하고 검증합니다.
  - 명령어: `gbs build -A x86_64 --include-all`
- `gtest_integration.md` 워크플로우를 참조하여 컴포넌트 단위 검증이 통과하는지 확인합니다.

## 3. Verify (기기 배포 및 검증)
작성 및 로컬 검증이 완료된 코드는 실제 타겟(Tizen Emulator 또는 실기기)에 배포하여 동작을 확인해야 합니다.
해당 작업은 `deploy_to_emulator.md` 워크플로우를 따릅니다.

1. **디바이스 연결 확인**
   - 명령어: `sdb devices`
   - 타겟 디바이스가 `device` 상태인지 확인합니다.

2. **권한 및 쓰기 권한 확보**
   - 명령어: `sdb root on`
   - 명령어: `sdb shell mount -o remount,rw /`

3. **RPM 패키지 배포 및 설치**
   - 명령어: `sdb push ~/GBS-ROOT/local/repos/tizen/x86_64/RPMS/tizenclaw-1.0.0-1.x86_64.rpm /tmp/`
   - 명령어: `sdb shell rpm -Uvh --force /tmp/tizenclaw-1.0.0-1.x86_64.rpm`

4. **데몬 재시작 및 상태 확인**
   - 명령어: `sdb shell systemctl daemon-reload`
   - 명령어: `sdb shell systemctl restart tizenclaw`
   - 명령어: `sdb shell systemctl status tizenclaw -l`

## 4. Commit (작업 완료)
모든 검증이 끝났다면 `commit_guidelines.md`에 맞추어 `git commit`을 수행하여 작업을 마무리합니다.
