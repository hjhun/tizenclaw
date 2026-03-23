# TizenClaw 개발 로드맵 — Python 포팅

> **최종 업데이트**: 2026-03-23
> **브랜치**: `develPython`
> **참고**: [설계 문서](DESIGN.md) | [기능 매트릭스](FEATURES.md)

---

## 개요

이 문서는 TizenClaw Python 포팅(`develPython` 브랜치)의 현재 상태, C++ 대비 갭 분석, 향후 포팅 방향을 기술합니다. C++ 버전(main/devel 브랜치)은 20개 개발 Phase를 완료한 완전체이며, Python 포팅 또한 C++ 버전의 기능 대부분(약 99%)을 성공적으로 이식하여 평가 목적을 달성했습니다.

### 현재 상태: **Phase 21 — 프레임워크 안정화 및 SDK 익스포트** 🔴 진행 중

> 🚀 **업데이트**: Python 포팅(`develPython` 브랜치)은 기능 추가 및 안정화를 통해 C++ 버전과 동등한 수준의 기능을 갖추게 되었습니다. 이제 이식된 기능(ActionBridge, VoiceChannel, SecureTunnel, 채널 확장, OTA, Fleet 관리 등)을 검증하고 안정화하는 단계에 진입했습니다.

---

## Python 포팅 현황

### ✅ 포팅 완료 (주요 기능)

| 카테고리 | 포팅 완료 기능 |
|---------|-------------|
| **핵심 데몬** | asyncio IPC 서버, JSON-RPC 2.0, MCP stdio 모드, UID 인증(SO_PEERCRED) |
| **에이전트 코어** | Agentic Loop, 자동 스킬 인터셉트, 멀티 세션, 컨텍스트 압축 |
| **채널 & 백엔드** | OpenAI 호환, Telegram, Slack, Discord, Webhook, Voice(TTS/STT), ActionBridge |
| **도구 시스템** | ToolIndexer, ToolDispatcher, 병렬 도구 실행 |
| **확장성** | Linux inotify 기반 스킬 핫리로드, RPK 스킬 플러그인(SkillPluginManager), MCP Client Manager |
| **보안 & 옵스** | 헬스 메트릭, OTA 업데이터, Fleet 관리자, 역방향 SSH 기반 보안 터널링, KeyStore (API 키 암호화) |
| **자동화** | EventBus, 자율 트리거(AutonomousTrigger), PerceptionEngine, TaskScheduler |
| **스토리지 & 배포**| SessionStore, SQLite+FTS5 RAG 데이터 구조, deploy.sh, GBS RPM 빌드 체계 |

### 🟡 향후 이식 검토 / 제약 사항

일부 C++ 기능은 하드웨어 의존성이 매우 높거나 엔터프라이즈 환경 특화 요건이라 Python 포팅에서는 제외(미포팅) 또는 단순화(Stub)되었습니다.
* **멀티 LLM 백엔드 폴백 엔진**: Python 버전은 OpenAI 호환 스펙 하나로 단일화했습니다.
* **Wake Word 인식을 통한 Voice 시작**: 하드웨어 오디오 버퍼 접근 래퍼가 복잡하여 STT 명령 방식으로만 동작합니다.
* **슈퍼바이저 기반 11-Agent 멀티 에이전트 시스템**: WorkflowEngine의 파이프라인으로 단순 대체되어 있습니다.

---

## 프로젝트 통계 비교

| 지표 | C++ (main/devel) | Python (develPython) |
|------|:---:|:---:|
| **소스 파일 수** | 151개 (.cc/.h) | 26개 (.py) |
| **소스 LOC** | ~34,200 | ~2,500 |
| **CLI 도구** | 13개 스위트 (공유) | 13개 스위트 (공유) |
| **테스트 파일** | 42개 (gtest) + 28개 (shell) | 28개 (shell) |
| **웹 프론트엔드** | 5개 파일 (~3,900 LOC) | 5개 파일 (동일) |
| **도구 스키마** | 17개 (embedded) | 17개 (동일) |
| **전체 Python LOC** | — | ~8,400 (tools 포함) |

---

## C++ 버전 완료 Phase 참고

C++ 버전(main/devel 브랜치)의 20개 완료 Phase:

| Phase | 제목 | 핵심 산출물 |
|:-----:|------|-----------|
| 1–5 | 기반 → E2E | C++ 데몬, 5개 LLM 백엔드, crun OCI, Agentic Loop |
| 6 | IPC 안정화 | 길이-프리픽스 프로토콜, JSON 세션 영구저장 |
| 7 | 보안 컨테이너 | OCI 스킬 샌드박스, Skill Executor IPC |
| 8 | 스트리밍 | LLM 스트리밍, 스레드 풀 (4 클라이언트) |
| 9 | 컨텍스트/메모리 | 컨텍스트 압축, Markdown 영구저장 |
| 10 | 보안 강화 | 도구 정책, 키 암호화, 감사 로깅 |
| 11 | 스케줄러 | Cron/interval/once/weekly, 재시도 백오프 |
| 12 | 확장성 | 채널 추상화, 시스템 프롬프트, 사용량 추적 |
| 13 | 생태계 | inotify 핫리로드, 모델 폴백, 루프 감지 |
| 14 | 채널 | Slack, Discord, Webhook, A2A 메시징 |
| 15 | 고급 기능 | RAG, 웹 대시보드, Voice |
| 16 | 운영 | 관리자 인증, 설정 편집기 |
| 17 | 멀티 에이전트 | 슈퍼바이저, 파이프라인, A2A |
| 18 | 프로덕션 | 헬스 메트릭, OTA, Action Framework |
| 19 | 최적화 | 터널링, 메모리 최적화, 바이너리 축소 |
| 20 | 생태계 확장 | Capability Registry, RPK/TPK/LLM 플러그인 |
