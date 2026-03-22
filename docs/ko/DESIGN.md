# TizenClaw 설계 문서

> **최종 업데이트**: 2026-03-22
> **버전**: 3.0

---

## 목차

- [1. 개요](#1-개요)
- [2. 시스템 아키텍처](#2-시스템-아키텍처)
- [3. 핵심 모듈](#3-핵심-모듈)
- [4. LLM 백엔드 계층](#4-llm-백엔드-계층)
- [5. 통신 채널](#5-통신-채널)
- [6. 컨테이너 및 스킬 실행](#6-컨테이너-및-스킬-실행)
- [7. 보안 아키텍처](#7-보안-아키텍처)
- [8. 데이터 영구 저장](#8-데이터-영구-저장)
- [9. 멀티 에이전트 시스템](#9-멀티-에이전트-시스템)
- [10. 퍼셉션 및 이벤트 시스템](#10-퍼셉션-및-이벤트-시스템)
- [11. RAG 및 시맨틱 검색](#11-rag-및-시맨틱-검색)
- [12. 확장성 및 플러그인 시스템](#12-확장성-및-플러그인-시스템)
- [13. 웹 대시보드](#13-웹-대시보드)
- [14. 인프라 서비스](#14-인프라-서비스)
- [15. 설계 원칙](#15-설계-원칙)

---

## 1. 개요

**TizenClaw**는 Tizen Embedded Linux 플랫폼에 최적화된 네이티브 C++ AI 에이전트 **데몬**입니다. **systemd 서비스**로 백그라운드에서 실행되며, 7개 이상의 통신 채널(Telegram, Slack, Discord, MCP, Webhook, Voice, Web Dashboard)을 통해 사용자 프롬프트를 수신하고, 설정 가능한 LLM 백엔드(Gemini, OpenAI, Anthropic, xAI, Ollama + RPK 플러그인)를 통해 해석한 후, OCI 컨테이너 내부의 샌드박스된 Python 스킬과 **Tizen Action Framework**를 통해 디바이스 레벨 작업을 수행합니다.

Tizen의 보안 정책(SMACK, DAC, kUEP) 하에서 안전하고 확장 가능한 Agent-Skill 상호작용 환경을 구축하며, 멀티 에이전트 협조, 스트리밍 응답, 암호화된 자격증명 저장, 구조화된 감사 로깅 등 엔터프라이즈급 기능을 제공합니다.

### 시스템 환경

| 속성 | 세부사항 |
|------|---------|
| **OS** | Tizen Embedded Linux (Tizen 10.0+) |
| **런타임** | systemd 데몬 + 소켓 활성화 컴패니언 서비스 |
| **보안** | SMACK + DAC 적용, kUEP (Kernel Unprivileged Execution Protection) 활성화 |
| **언어** | C++20 (데몬), Python 3.x (스킬) |
| **바이너리 크기** | ~812KB (armv7l, strip 후) |
| **유휴 메모리** | ~8.5MB PSS |

### 설계 목표

1. **저부하(Low Footprint)** — 임베디드 디바이스를 위한 메모리/CPU 최소화
2. **기본 보안** — 컨테이너 격리, 자격증명 암호화, 도구 정책
3. **확장성** — LLM 백엔드, 채널, 스킬의 플러그인 아키텍처 (재컴파일 불필요)
4. **플랫폼 통합** — Action Framework 및 ctypes FFI를 통한 깊은 Tizen C-API 접근
5. **멀티 프로바이더 LLM** — 벤더 독립적 AI와 자동 장애 전환

---

## 2. 시스템 아키텍처

### 상위 아키텍처

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              외부 채널                                      │
│  Telegram · Slack · Discord · MCP · Webhook · Voice (STT/TTS) · Dashboard  │
└──────┬──────────┬────────────┬─────────┬──────────────────────────┬─────────┘
       │          │            │         │                          │
       ▼          ▼            │         ▼                         ▼
┌──────────────────────────────┼────────────────────────────────────────────────┐
│  TizenClaw Daemon (systemd)  │                                               │
│                              │                                               │
│  ┌────────────────┐   ┌──────┴──────┐   ┌──────────────────────────────────┐ │
│  │ ChannelRegistry│──▶│  IPC 서버   │   │       LLM 백엔드 계층           │ │
│  └────────────────┘   │ (JSON-RPC   │   │  ┌────────┐ ┌────────┐          │ │
│                       │  2.0 / UDS) │   │  │ Gemini │ │ OpenAI │          │ │
│                       └──────┬──────┘   │  └────────┘ └────────┘          │ │
│                              │          │  ┌────────┐ ┌────────┐ ┌──────┐ │ │
│                              ▼          │  │Anthropic│ │ Ollama │ │Plugin│ │ │
│                       ┌─────────────┐   │  └────────┘ └────────┘ └──────┘ │ │
│                       │  AgentCore  │──▶│         (우선순위 기반)           │ │
│                       │(Agentic Loop│   └──────────────────────────────────┘ │
│                       │ + Streaming)│                                        │
│                       └──┬───┬───┬──┘                                       │
│                ┌─────────┘   │   └──────────┐                               │
│                ▼             ▼               ▼                              │
│  ┌──────────────────┐ ┌───────────┐ ┌──────────────┐  ┌────────────────┐   │
│  │ ContainerEngine  │ │ Session   │ │ ActionBridge │  │ EmbeddingStore │   │
│  │   (crun OCI)     │ │ Store     │ │(Action FW)   │  │  (SQLite RAG)  │   │
│  └────┬────────┬────┘ └───────────┘ └──────┬───────┘  └────────────────┘   │
│       │        │                           │                                │
│       │        │     ┌─────────────────┐   │   ┌────────────────────────┐   │
│       │        │     │ TaskScheduler   │   │   │ WebDashboard (:9090)  │   │
│       │        │     └─────────────────┘   │   └────────────────────────┘   │
└───────┼────────┼───────────────────────────┼────────────────────────────────┘
        │        │                           │
        ▼        ▼                           ▼
┌──────────┐ ┌─────────────────┐   ┌──────────────────────┐
│Tool Exec │ │보안 컨테이너    │   │ Tizen Action          │
│(소켓     │ │    (crun)       │   │ Framework             │
│ 활성화)  │ │                 │   │                       │
│          │ │ Python 스킬     │   │ 디바이스별 액션       │
│CLI 실행  │ │ (샌드박스)      │   │ (자동 발견)           │
│ via IPC  │ │ 13 CLI 도구     │   │                       │
└──────────┘ └─────────────────┘   └──────────────────────┘
```

---

## 3. 핵심 모듈

### 3.1 데몬 프로세스 (`tizenclaw.cc`)

| 책임 | 구현 |
|------|------|
| **systemd 통합** | `Type=simple` 서비스, `SIGINT`/`SIGTERM` 안전 종료 |
| **IPC 서버** | Abstract Unix Domain Socket (`\0tizenclaw.sock`), JSON-RPC 2.0, `[4바이트 길이][JSON]` 프레이밍 |
| **인증** | `SO_PEERCRED` 기반 UID 검증 (root, app_fw, system, developer) |
| **동시성** | 스레드 풀, `kMaxConcurrentClients = 4` |
| **채널 생명주기** | `ChannelFactory`가 `channels.json` → `ChannelRegistry`로 시작/정지 관리 |
| **C-API SDK** | `libtizenclaw` 내부 로직과 분리된 SDK 배포용 라이브러리 |

### 3.2 Agent Core (`agent_core.cc`)

**Agentic Loop**을 구현하는 핵심 오케스트레이션 엔진:

| 기능 | 세부사항 |
|------|---------|
| **반복 도구 호출** | LLM이 도구 호출 생성 → 실행 → 결과 피드백 → 반복 |
| **스트리밍 응답** | 청크 IPC 전달 (`stream_chunk` / `stream_end`) |
| **컨텍스트 압축** | 15턴 초과 시 가장 오래된 10턴을 LLM으로 요약 압축 |
| **엣지 메모리 관리** | `MaintenanceLoop`가 5분 유휴 시 `malloc_trim(0)` + `sqlite3_release_memory` 호출 |
| **멀티 세션** | 세션별 시스템 프롬프트와 히스토리 격리 |
| **백엔드 선택** | `SwitchToBestBackend()` 통합 우선순위 큐 (Plugin > active > fallback) |

### 3.3 도구 디스패처 (`tool_dispatcher.cc`)

AgentCore에서 분리된 모듈형 도구 디스패치:

- **O(1) 조회**: `std::unordered_map<string, ToolHandler>` 등록 도구
- **동적 폴백**: `action_*` 동적 이름 도구용 `starts_with` 매칭
- **스레드 안전**: 동시 읽기를 위한 `std::shared_mutex`
- **Capability 통합**: 모든 도구가 `CapabilityRegistry`에 `FunctionContract`로 등록

### 3.4 Capability Registry (`capability_registry.cc`)

모든 도구(내장, CLI 플러그인, RPK 스킬, 액션)의 통합 등록:

- 시작 시 21개 내장 capability 자동 등록
- `{{CAPABILITY_SUMMARY}}` placeholder로 LLM 시스템 프롬프트에 주입
- 카테고리/부작용/권한 기반 쿼리로 지능적 계획 수립 지원

### 3.5 Tizen Action Framework 브릿지 (`action_bridge.cc`)

| 기능 | 세부사항 |
|------|---------|
| **워커 스레드** | 전용 `tizen_core_task` 에서 Action C API 실행 |
| **스키마 동기화** | `SyncActionSchemas()` — 초기화 시 모든 액션 가져와 MD 파일 생성 |
| **이벤트 기반** | INSTALL/UNINSTALL/UPDATE 이벤트로 MD 파일 자동 갱신 |
| **Per-Action 도구** | 각 액션이 타입이 지정된 LLM 도구 (예: `action_<name>`) |
| **실행** | `action_client_execute`, JSON-RPC 2.0 형식 |

### 3.6 태스크 스케줄러 (`task_scheduler.cc`)

| 스케줄 타입 | 예시 |
|------------|------|
| `daily HH:MM` | `daily 09:00` |
| `interval Ns/Nm/Nh` | `interval 30m` |
| `once YYYY-MM-DD HH:MM` | `once 2026-04-01 12:00` |
| `weekly DAY HH:MM` | `weekly MON 08:00` |

- `AgentCore::ProcessPrompt()` 직접 호출 (IPC 슬롯 미소비)
- YAML frontmatter Markdown 영구 저장
- 지수 백오프 재시도 (최대 3회)

---

## 4. LLM 백엔드 계층

`LlmBackend` 인터페이스를 통한 프로바이더 불가지 추상화:

| 백엔드 | 소스 | 기본 모델 | 스트리밍 | 토큰 카운팅 |
|--------|------|----------|:--------:|:----------:|
| Gemini | `gemini_backend.cc` | `gemini-2.5-flash` | ✅ | ✅ |
| OpenAI | `openai_backend.cc` | `gpt-4o` | ✅ | ✅ |
| xAI (Grok) | `openai_backend.cc` | `grok-3` | ✅ | ✅ |
| Anthropic | `anthropic_backend.cc` | `claude-sonnet-4-20250514` | ✅ | ✅ |
| Ollama | `ollama_backend.cc` | `llama3` | ✅ | ✅ |

### 핵심 설계 결정

- **팩토리 패턴**: `LlmBackendFactory::Create()` 인스턴스 생성
- **통합 우선순위 전환**: `active_backend`와 `fallback_backends`는 기본 우선순위 `1`. RPK 플러그인은 높은 우선순위(예: `10`)를 지정하여 설치 시 자동 라우팅, 제거 시 기본 백엔드로 폴백
- **시스템 프롬프트**: 4단계 fallback (config → 파일 → 기본 파일 → 하드코딩)
- **동적 placeholder**: `{{AVAILABLE_TOOLS}}`, `{{CAPABILITY_SUMMARY}}`, `{{MEMORY_CONTEXT}}`

---

## 5. 통신 채널

통합 `Channel` 인터페이스:

| 채널 | 프로토콜 | 아웃바운드 | 라이브러리 |
|------|---------|:--------:|-----------|
| **Telegram** | Bot API Long-Polling | ✅ | libcurl |
| **Slack** | Socket Mode (WebSocket) | ✅ | libwebsockets |
| **Discord** | Gateway WebSocket | ✅ | libwebsockets |
| **MCP** | stdio JSON-RPC 2.0 | ❌ | 내장 |
| **Webhook** | HTTP 인바운드 | ❌ | libsoup |
| **Voice** | Tizen STT/TTS C-API | ✅ | 조건부 컴파일 |
| **Web Dashboard** | libsoup SPA (port 9090) | ❌ | libsoup |
| **Plugin (.so)** | C API (`tizenclaw_channel.h`) | 선택 | dlopen |

### 아웃바운드 메시징

- `SendTo(channel, text)` — 특정 채널로 전송
- `Broadcast(text)` — 모든 아웃바운드 가능 채널에 전송
- IPC 메서드 `send_to` — `tizenclaw-cli --send-to` 통한 외부 트리거
- `AutonomousTrigger::Notify()` — 이벤트 기반 자율 알림

---

## 6. 컨테이너 및 스킬 실행

### 컨테이너 엔진 (`container_engine.cc`)

| 기능 | 세부사항 |
|------|---------|
| **런타임** | `crun` 1.26 (RPM 패키징 시 소스 빌드) |
| **격리** | PID, Mount, User 네임스페이스 |
| **폴백** | cgroup 미사용 시 `unshare + chroot` |
| **스킬 IPC** | Unix Domain Socket을 통한 길이-프리픽스 JSON |
| **도구 실행기 IPC** | Abstract namespace socket (`@tizenclaw-tool-executor.sock`) |
| **소켓 활성화** | systemd `LISTEN_FDS`/`LISTEN_PID`로 온디맨드 시작 |
| **바인드 마운트** | Tizen C-API 접근을 위한 `/usr/bin`, `/usr/lib`, `/usr/lib64`, `/lib64` |

### 도구 스키마 디스커버리

| 소스 | 경로 | 설명 |
|------|------|------|
| **내장 도구** | `tools/embedded/*.md` | 17개 MD 파일 |
| **Action 도구** | `tools/actions/*.md` | Tizen Action Framework에서 자동 동기화 |
| **CLI 도구** | `tools/cli/*/.tool.md` | TPK 패키지에서 symlink |

모든 MD 내용은 프롬프트 빌드 시 스캔되어 `{{AVAILABLE_TOOLS}}`에 주입됩니다.

---

## 7. 보안 아키텍처

| 컴포넌트 | 파일 | 기능 |
|---------|------|------|
| **KeyStore** | `key_store.cc` | 디바이스 바인딩 API 키 암호화 (GLib SHA-256 + XOR, `/etc/machine-id`) |
| **ToolPolicy** | `tool_policy.cc` | 스킬별 `risk_level`, 루프 감지 (3회 반복 차단), idle 진행 체크 |
| **AuditLogger** | `audit_logger.cc` | Markdown 테이블 감사 파일, 일별 로테이션, 5MB 제한 |
| **UID 인증** | `tizenclaw.cc` | `SO_PEERCRED` IPC 발신자 검증 |
| **관리자 인증** | `web_dashboard.cc` | 세션 토큰 + SHA-256 비밀번호 해싱 |
| **Webhook 인증** | `webhook_channel.cc` | HMAC-SHA256 서명 검증 |

---

## 8. 데이터 영구 저장

모든 저장소는 **Markdown + YAML frontmatter** 사용 (RAG용 SQLite 제외):

```
/opt/usr/share/tizenclaw/
├── sessions/{YYYY-MM-DD}-{id}.md    ← 대화 히스토리
├── logs/{YYYY-MM-DD}.md             ← 일별 스킬 실행 로그
├── usage/                           ← 토큰 사용량
├── audit/YYYY-MM-DD.md              ← 감사 추적
├── tasks/task-{id}.md               ← 예약 태스크
├── tools/                           ← 도구 스키마 (actions, embedded, cli)
├── memory/                          ← 영속 메모리 (장기/에피소드/단기)
├── config/                          ← 설정 파일
├── pipelines/                       ← 파이프라인 정의
├── workflows/                       ← 워크플로우 정의
└── knowledge/embeddings.db          ← SQLite 벡터 저장소 (RAG)
```

### 메모리 시스템

| 타입 | 경로 | 보존 기간 | 최대 크기 |
|------|------|----------|----------|
| **단기** | `short-term/{session_id}/` | 24시간, 세션당 최대 50개 | - |
| **장기** | `long-term/{date}-{title}.md` | 영구 | 2KB/파일 |
| **에피소드** | `episodic/{date}-{skill}.md` | 30일 | 2KB/파일 |
| **요약** | `memory.md` | idle 시 자동 갱신 | 8KB |

- LLM 도구: `remember` (저장), `recall` (검색), `forget` (삭제)
- 시스템 프롬프트: `{{MEMORY_CONTEXT}}` placeholder로 `memory.md` 주입

---

## 9. 멀티 에이전트 시스템

### 11-Agent MVP 세트

| 카테고리 | 에이전트 | 책임 |
|----------|---------|------|
| **이해** | Input Understanding | 채널 간 사용자 입력을 통합 인텐트로 표준화 |
| **인식** | Environment Perception | 이벤트 버스 구독, 공통 상태 스키마 유지 |
| **기억** | Session / Context | 단기/장기/에피소드 메모리 관리 |
| **판단** | Planning (오케스트레이터) | Capability Registry 기반 목표 분해 |
| **실행** | Action Execution | 컨테이너 스킬 및 Action Framework 호출 |
| **보호** | Policy / Safety | 도구 정책 및 시스템 보호 적용 |
| **유틸리티** | Knowledge Retrieval | EmbeddingStore를 통한 RAG 시맨틱 검색 |
| **모니터링** | Health Monitoring | PSS 메모리, 업타임, 컨테이너 상태 |
| | Recovery | 실패 분석 및 LLM 기반 오류 수정 |
| | Logging / Trace | 디버깅 및 감사 컨텍스트 중앙화 |

### 슈퍼바이저 패턴

`SupervisorEngine`이 목표 분해 → 전문 역할 에이전트에 위임 → 결과 검증:

- `agent_roles.json`으로 설정 가능
- 내장 도구: `run_supervisor`, `list_agent_roles`

### A2A (Agent-to-Agent) 프로토콜

크로스 디바이스 에이전트 협조:

- WebDashboard HTTP 서버의 A2A 엔드포인트
- Agent Card 디스커버리 (`.well-known/agent.json`)
- 태스크 생명주기: `submitted` → `working` → `completed` / `failed` / `cancelled`
- `a2a_config.json`을 통한 Bearer 토큰 인증

---

## 10. 퍼셉션 및 이벤트 시스템

### 이벤트 버스 (`event_bus.cc`)

- 세분화된 이벤트: `sensor.changed`, `network.disconnected`, `app.started` 등
- 폴링 방지 — CPU 효율적 실시간 상태 업데이트
- `EventAdapterManager`가 어댑터 생명주기 관리

### 이벤트 어댑터

| 어댑터 | 이벤트 |
|--------|--------|
| **App Lifecycle** | `app.launched`, `app.terminated` |
| **Recent App** | `app.recent` |
| **Package** | `package.installed`, `package.uninstalled` |
| **System Event** | `battery.low`, `wifi.changed` |

### 자율 트리거 (`autonomous_trigger.cc`)

- EventBus 시스템 이벤트 구독
- `autonomous_trigger.json`으로 규칙 정의
- 트리거 폭주 방지용 쿨다운 설정
- LLM이 컨텍스트 평가 후 자율 액션 실행

### 공통 상태 스키마

LLM 소비를 위한 정규화된 JSON 스키마:

- `DeviceState`: 활성 기능, 모델명
- `RuntimeState`: 네트워크 상태, 메모리 압박, 전원 모드
- `UserState`: 로캘, 선호설정, 역할
- `TaskState`: 현재 목표, 활성 단계

---

## 11. RAG 및 시맨틱 검색

### EmbeddingStore (`embedding_store.cc`)

| 기능 | 구현 |
|------|------|
| **저장소** | SQLite + FTS5 가상 테이블 |
| **하이브리드 검색** | BM25 키워드(FTS5) + 벡터 코사인을 RRF(k=60)로 결합 |
| **토큰 예산** | `EstimateTokens()` 근사치 (단어 × 1.3) |
| **임베딩 API** | Gemini, OpenAI, Ollama |
| **온디바이스** | ONNX Runtime `all-MiniLM-L6-v2` (384차원, 지연 로드) |
| **멀티 DB** | 다중 지식 데이터베이스 동시 연결 |
| **폴백** | FTS5 미사용 시 벡터 전용 검색 |

---

## 12. 확장성 및 플러그인 시스템

### 3대 확장 포인트

| 메커니즘 | 패키지 타입 | 런타임 | 용도 |
|---------|:---:|:---:|------|
| **RPK 스킬 플러그인** | RPK | Python (OCI 샌드박스) | 샌드박스된 디바이스 분석 도구 |
| **CLI 도구 플러그인** | TPK | 네이티브 바이너리 (호스트) | 권한이 필요한 Tizen C-API 접근 |
| **LLM 백엔드 플러그인** | RPK | 공유 라이브러리 | 커스텀 LLM 백엔드 |

모든 플러그인은 보안을 위해 **플랫폼 레벨 인증서 서명**이 필요합니다.

### 메타데이터 파서 플러그인

3개의 `pkgmgr-metadata-plugin` 공유 라이브러리가 패키지 설치 시 보안을 적용:

| 플러그인 | 메타데이터 키 | 검증 |
|---------|-------------|------|
| `tizenclaw-metadata-skill-plugin.so` | `tizenclaw/skill` | 플랫폼 인증서 |
| `tizenclaw-metadata-cli-plugin.so` | `tizenclaw/cli` | 플랫폼 인증서 |
| `tizenclaw-metadata-llm-backend-plugin.so` | `tizenclaw/llm-backend` | 플랫폼 인증서 |

---

## 13. 웹 대시보드

내장 관리 대시보드 (`web_dashboard.cc`):

| 기능 | 세부사항 |
|------|---------|
| **서버** | libsoup `SoupServer`, 포트 9090 |
| **프론트엔드** | 다크 글래스모피즘 SPA (HTML + CSS + JS) |
| **관리자 인증** | SHA-256 비밀번호 해싱 + 세션 토큰 |
| **설정 편집기** | 7개+ 설정 파일 인브라우저 편집, 백업-온-라이트 |

### REST API

| 엔드포인트 | 메서드 | 설명 |
|----------|--------|------|
| `/api/sessions` | GET | 활성 세션 목록 |
| `/api/tasks` | GET | 예약 태스크 목록 |
| `/api/logs` | GET | 실행 로그 조회 |
| `/api/chat` | POST | 웹 인터페이스 프롬프트 전송 |
| `/api/config` | GET/POST | 설정 파일 읽기/쓰기 |
| `/api/metrics` | GET | Prometheus 스타일 헬스 메트릭 |

---

## 14. 인프라 서비스

| 서비스 | 설명 |
|--------|------|
| **HttpClient** | libcurl POST, 지수 백오프, SSL CA 자동 발견 |
| **SkillWatcher** | inotify 모니터링, 500ms 디바운싱, 핫리로드 |
| **FleetAgent** | 엔터프라이즈 멀티 디바이스 관리 (heartbeat, 등록) |
| **OTA Updater** | HTTP 풀 기반 스킬 업데이트, 롤백 지원 |
| **TunnelManager** | ngrok 보안 터널링 |
| **HealthMonitor** | CPU, 메모리, 업타임, 요청 수 메트릭 |

---

## 15. 설계 원칙

### 임베디드 우선 설계

1. **선택적 컨텍스트 주입** — 원시 데이터가 아닌 해석된 상태만 LLM에 전달
2. **인식-실행 분리** — Perception Agent가 상태를 읽고, Execution Agent가 실행
3. **지연 초기화** — 무거운 서브시스템(임베딩 모델, ONNX Runtime)은 첫 사용 시 로드
4. **적극적 메모리 회수** — idle 시 `malloc_trim(0)` + SQLite 캐시 플러시

### 스키마-실행 분리

- Markdown 스키마 파일은 LLM 컨텍스트만 제공
- 실행 로직은 ToolDispatcher가 독립적으로 처리
- 코드 변경 없이 스키마 업데이트 가능

### 설정 기반 확장성

- 채널 활성화: `channels.json`
- LLM 백엔드: `llm_config.json`
- 도구 정책: `tool_policy.json`
- 에이전트 역할: `agent_roles.json`
- 모든 항목 Web Dashboard에서 편집 가능

### Anthropic 표준 호환

- 스킬은 `SKILL.md` 형식 (YAML frontmatter + JSON schemas)
- 내장 MCP 클라이언트로 외부 MCP 도구 서버 연결
- 내장 MCP 서버로 Claude Desktop 통합
