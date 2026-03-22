# TizenClaw 개발 로드맵 — Python 포팅

> **최종 업데이트**: 2026-03-23
> **브랜치**: `develPython`
> **참고**: [설계 문서](DESIGN.md) | [기능 매트릭스](FEATURES.md)

---

## 개요

이 문서는 TizenClaw Python 포팅(`develPython` 브랜치)의 현재 상태, C++ 대비 갭 분석, 향후 포팅 방향을 기술합니다. C++ 버전(main/devel 브랜치)은 20개 개발 Phase를 완료한 완전체이며, Python 포팅은 핵심 에이전트 기능을 중심으로 선택적 재구현을 진행 중입니다.

### 현재 상태: **Python 핵심 포팅 완료, 기능 확장 중** 🟡

---

## Python 포팅 현황

### ✅ 포팅 완료

| 카테고리 | 포팅 완료 기능 |
|---------|-------------|
| **핵심 데몬** | asyncio IPC 서버, JSON-RPC 2.0, MCP stdio 모드 |
| **에이전트 코어** | Agentic Loop (최대 10회), 자동 스킬 인터셉트, 멀티 세션 |
| **LLM 백엔드** | OpenAI 호환 (gpt-4o), urllib.request + asyncio.to_thread |
| **도구 시스템** | ToolIndexer (.tool.md/.skill.md/.mcp.json), ToolDispatcher |
| **CLI 도구** | 13개 네이티브 스위트 (C++과 공유, 34+ 디바이스 스킬) |
| **내장 스키마** | 17개 embedded MD 도구 스키마 |
| **스토리지** | SessionStore (Markdown), MemoryStore (3단계), EmbeddingStore (SQLite+FTS5) |
| **워크플로우** | WorkflowEngine (Markdown 파싱, 변수 보간) |
| **스케줄러** | TaskScheduler (asyncio, 4가지 스케줄 타입) |
| **Tizen 연동** | dlog 핸들러 (ctypes), 시스템 이벤트 어댑터 (ctypes) |
| **CLI 클라이언트** | tizenclaw-cli (C++과 동일 플래그) |
| **배포** | systemd 서비스/소켓, deploy.sh, GBS RPM |
| **테스트** | 28개 Shell 검증 테스트 |

### 🔴 미포팅 (C++ 대비 갭)

아래는 C++ 버전에 존재하나 Python에 미구현된 기능들을 **영향도**와 **포팅 난이도**로 분류한 것입니다:

#### 높은 영향도

| 기능 | C++ 구현 | 포팅 난이도 | 설명 |
|------|---------|:---:|------|
| **멀티 LLM 백엔드** | Gemini, Anthropic, xAI, Ollama | 중간 | `LlmBackend` ABC 확장으로 각 백엔드 추가 |
| **우선순위 전환/폴백** | 통합 우선순위 큐 | 중간 | 백엔드 추가 후 팩토리 패턴 구현 |
| **통신 채널** | Telegram, Slack, Discord, Webhook, Voice | 높음 | 각 채널 프로토콜(WebSocket, Long-Poll) 재구현 필요 |
| **UID 인증** | `SO_PEERCRED` | 낮음 | Python `socket.SO_PEERCRED` 지원 |
| **도구 실행 정책** | ToolPolicy (위험도, 루프 감지) | 낮음 | 설정 파일 로더 + 간단한 룰 엔진 |
| **멀티 에이전트** | 11-Agent MVP, SupervisorEngine | 높음 | 에이전트 역할 시스템 + 세션 협조 |

#### 중간 영향도

| 기능 | C++ 구현 | 포팅 난이도 | 설명 |
|------|---------|:---:|------|
| **API 키 암호화** | GLib SHA-256 + XOR | 낮음 | Python `hashlib` + XOR |
| **스트리밍 응답** | SSE 청크 파싱 | 중간 | readline 기반 스트리밍 파서 |
| **컨텍스트 압축** | LLM 기반 요약 | 낮음 | 히스토리 카운트 → LLM 요약 호출 |
| **감사 로깅** | AuditLogger (Markdown 테이블) | 낮음 | 파일 추가 기록 |
| **이벤트 버스** | Pub/Sub | 중간 | asyncio 이벤트 기반 |
| **자율 트리거** | AutonomousTrigger | 중간 | 이벤트 버스 + 규칙 엔진 |
| **Capability Registry** | FunctionContract | 중간 | 도구 메타데이터 확장 |
| **하이브리드 RAG** | BM25 + 벡터 RRF 완전 구현 | 중간 | FTS5 쿼리 + RRF 스코어링 |
| **스킬 핫리로드** | inotify | 낮음 | `watchdog` 또는 polling |

#### 낮은 영향도

| 기능 | C++ 구현 | 포팅 난이도 | 설명 |
|------|---------|:---:|------|
| **A2A 프로토콜** | HTTP JSON-RPC | 높음 | 크로스 디바이스 통신 |
| **Action Framework** | ActionBridge | 높음 | Tizen C-API 전용 |
| **OTA 업데이터** | HTTP 풀 + 롤백 | 중간 | HTTP 클라이언트 + 버전 비교 |
| **Fleet 관리** | FleetAgent | 높음 | 엔터프라이즈 전용 |
| **보안 터널링** | ngrok TunnelManager | 낮음 | subprocess 래퍼 |
| **RPK/TPK 플러그인** | SkillPluginManager, CliPluginManager | 높음 | pkgmgrinfo C-API 의존 |
| **온디바이스 OCR** | PaddleOCR PP-OCRv3 | 높음 | ONNX + 전처리 파이프라인 |
| **헬스 메트릭** | Prometheus 스타일 | 낮음 | HTTP 엔드포인트 + JSON |

---

## 권장 포팅 우선순위

### Phase P1: 핵심 완성도 (권장 즉시)

| 우선순위 | 기능 | 예상 LOC | 이유 |
|:---:|------|:---:|------|
| 1 | UID 인증 (`SO_PEERCRED`) | ~30 | 보안 필수, Python stdlib 지원 |
| 2 | 도구 실행 정책 (ToolPolicy) | ~80 | 안전한 도구 실행 보장 |
| 3 | 스트리밍 응답 | ~60 | UX 개선, readline 기반 |
| 4 | API 키 암호화 | ~40 | 보안, hashlib 사용 |
| 5 | 감사 로깅 (AuditLogger) | ~50 | 운영 추적 |
| 6 | 컨텍스트 압축 | ~30 | 장시간 대화 메모리 관리 |

### Phase P2: LLM 확장 (중기)

| 우선순위 | 기능 | 예상 LOC | 이유 |
|:---:|------|:---:|------|
| 1 | Gemini 백엔드 | ~100 | 주요 LLM 프로바이더 |
| 2 | Anthropic 백엔드 | ~100 | Claude 지원 |
| 3 | Ollama 백엔드 | ~80 | 로컬 LLM 지원 |
| 4 | 백엔드 우선순위 전환 | ~60 | 다중 백엔드 활용 |
| 5 | 하이브리드 RAG 완성 | ~80 | 검색 품질 향상 |

### Phase P3: 채널 및 자동화 (장기)

| 우선순위 | 기능 | 예상 LOC | 이유 |
|:---:|------|:---:|------|
| 1 | Telegram 채널 | ~150 | 가장 많이 사용되는 채널 |
| 2 | 이벤트 버스 | ~100 | 자동화 기반 |
| 3 | 자율 트리거 | ~80 | 프로액티브 에이전트 |
| 4 | 슈퍼바이저 에이전트 | ~120 | 멀티 에이전트 |

---

## 프로젝트 통계 비교

| 지표 | C++ (main/devel) | Python (develPython) |
|------|:---:|:---:|
| **소스 파일 수** | 151개 (.cc/.h) | 20개 (.py) |
| **소스 LOC** | ~34,200 | ~1,800 |
| **CLI 도구** | 13개 스위트 (공유) | 13개 스위트 (공유) |
| **테스트 파일** | 42개 (gtest) + 28개 (shell) | 28개 (shell) |
| **웹 프론트엔드** | 5개 파일 (~3,900 LOC) | 5개 파일 (동일) |
| **도구 스키마** | 17개 (embedded) | 17개 (동일) |
| **전체 Python LOC** | — | ~7,600 (tools 포함) |

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
