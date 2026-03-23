# TizenClaw 설계 문서 — Python 포팅

> **최종 업데이트**: 2026-03-23
> **버전**: 4.0 (`develPython` 브랜치)

---

## 목차

- [1. 개요](#1-개요)
- [2. 시스템 아키텍처](#2-시스템-아키텍처)
- [3. 핵심 모듈](#3-핵심-모듈)
- [4. LLM 백엔드 계층](#4-llm-백엔드-계층)
- [5. 통신 및 IPC](#5-통신-및-ipc)
- [6. 컨테이너 및 스킬 실행](#6-컨테이너-및-스킬-실행)
- [7. 데이터 영구 저장](#7-데이터-영구-저장)
- [8. RAG 및 시맨틱 검색](#8-rag-및-시맨틱-검색)
- [9. 워크플로우 엔진](#9-워크플로우-엔진)
- [10. 태스크 스케줄러](#10-태스크-스케줄러)
- [11. Tizen 네이티브 연동](#11-tizen-네이티브-연동)
- [12. 설계 원칙](#12-설계-원칙)
- [부록: C++ 대비 비교](#부록-c-대비-비교)

---

## 1. 개요

**TizenClaw (Python Port)**는 `develPython` 브랜치에서 진행 중인 C++20 데몬의 **순수 Python 3 포팅** 프로젝트입니다. Tizen 임베디드 디바이스에서의 메모리, 속도, 저장소 풋프린트를 비교 평가하기 위해 전면 재작성되었습니다.

**systemd 서비스**로 백그라운드 실행되며, IPC(JSON-RPC 2.0 over Unix Domain Sockets)를 통해 사용자 프롬프트를 수신하고, OpenAI 호환 LLM 백엔드를 통해 의도를 해석한 후, 네이티브 CLI 도구 스위트를 통해 디바이스 레벨 작업을 수행합니다.

### 시스템 환경

| 속성 | C++ (main/devel) | Python (develPython) |
|------|:---:|:---:|
| **OS** | Tizen 10.0+ | Tizen 10.0+ |
| **언어** | C++20 | Python 3.x |
| **외부 의존성** | libcurl, libsoup, nlohmann/json 등 | **없음** (stdlib만 사용) |
| **HTTP 클라이언트** | libcurl | `urllib.request` |
| **IPC** | C++ 스레드 + UDS | `asyncio` Unix 소켓 |
| **LLM 백엔드** | 5개 (Gemini, OpenAI, Anthropic, xAI, Ollama) | 1개 (OpenAI 호환) |
| **컨테이너** | crun 1.26 (OCI) | `unshare` 폴백 |
| **바이너리 크기** | ~812KB | 해당 없음 (인터프리터) |

### 설계 목표

1. **제로 외부 의존성** — Python 표준 라이브러리만 사용하여 최대 이식성 확보
2. **C++ 동등성 평가** — 동일 IPC 프로토콜, 도구 스키마, CLI 인터페이스
3. **asyncio 우선** — 스레딩 없이 비동기 협력 동시성
4. **플랫폼 연동** — `ctypes` FFI를 통한 Tizen C-API 접근

---

## 2. 시스템 아키텍처

### 상위 아키텍처

```
┌──────────────────────────────────────────────────────────────────────┐
│                         외부 인터페이스                               │
│    tizenclaw-cli (Python)  ·  MCP stdio  ·  Web Dashboard (:9090)   │
└────────┬──────────────────────┬──────────────────────────┬──────────┘
         │                      │                          │
         ▼                      ▼                          ▼
┌──────────────────────────────────────────────────────────────────────┐
│  TizenClaw 데몬 (tizenclaw_daemon.py / systemd)                     │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  IPC 서버 (asyncio.start_unix_server)                         │  │
│  │  프로토콜: JSON-RPC 2.0, [4바이트 길이][JSON] 프레이밍         │  │
│  │  소켓: abstract namespace \0tizenclaw.sock                    │  │
│  └──────────────────────┬────────────────────────────────────────┘  │
│                          │                                           │
│  ┌───────────────────────▼───────────────────────────────────────┐  │
│  │  AgentCore (agent_core.py)                                    │  │
│  │  • Agentic Loop (최대 10회 도구 반복)                          │  │
│  │  • 자동 스킬 인터셉트 (LLM 우회 직접 실행)                     │  │
│  │  • asyncio.Lock을 통한 세션별 히스토리 격리                    │  │
│  └──┬──────────┬──────────┬──────────┬──────────┬───────────────┘  │
│     │          │          │          │          │                    │
│     ▼          ▼          ▼          ▼          ▼                   │
│  ToolIndex  ToolDisp   OpenAI    Session   Embedding               │
│  er         atcher     Backend   /Memory   Store                   │
│  (.md 스캔) (라우팅)   (urllib)  Store     (SQLite)                 │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  추가 모듈                                                    │  │
│  │  • WorkflowEngine — Markdown 기반 결정적 파이프라인            │  │
│  │  • TaskScheduler — asyncio 기반 cron/interval 스케줄링        │  │
│  │  • MemoryStore — 장기/에피소드/단기 메모리 (Markdown)          │  │
│  │  • TizenSystemEventAdapter — ctypes app_event 연동            │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────┬───────────────────────────────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────────────────────────────┐
│  ContainerEngine (container_engine.py)                                │
│  도구 실행기와 abstract UDS IPC 통신                                  │
│                                                                      │
│  ┌─────────────────────┐    ┌─────────────────────┐                 │
│  │ tizenclaw-tool-     │    │ tizenclaw-code-     │                 │
│  │ executor.py         │    │ sandbox.py          │                 │
│  │ (소켓 활성화)        │    │ (소켓 활성화)       │                 │
│  │ asyncio 서브프로세스 │    │ 스텁 리스너          │                 │
│  └─────────────────────┘    └─────────────────────┘                 │
│                                                                      │
│  13개 CLI 도구 스위트 (ctypes FFI → Tizen C-API)                     │
└──────────────────────────────────────────────────────────────────────┘
```

### 서비스 토폴로지

```
systemd
├── tizenclaw.service             (Type=simple, ExecStart=/usr/bin/tizenclaw)
├── tizenclaw-tool-executor.socket   (소켓 활성화)
│   └── tizenclaw-tool-executor.service
└── tizenclaw-code-sandbox.socket    (소켓 활성화)
    └── tizenclaw-code-sandbox.service
```

---

## 3. 핵심 모듈

### 3.1 데몬 프로세스 (`tizenclaw_daemon.py`)

| 책임 | 구현 |
|------|------|
| **systemd 통합** | `Type=simple` 서비스, `KeyboardInterrupt` 안전 종료 |
| **IPC 서버** | `asyncio.start_unix_server`, abstract 소켓 (`\0tizenclaw.sock`) |
| **프로토콜** | JSON-RPC 2.0, `[4바이트 네트워크 엔디안 길이][JSON]` 프레이밍 |
| **페이로드 보호** | 10MB 초과 페이로드 거부 |
| **MCP 모드** | `--mcp-stdio` 플래그로 Claude Desktop 연동 (stdin/stdout JSON-RPC) |
| **RPC 메서드** | `prompt`, `connect_mcp`, `list_mcp`, `list_agents` |

> **C++ 대비 부족**: UID 인증 (`SO_PEERCRED`), 스레드 풀 동시성 (4 클라이언트), `libtizenclaw` SDK 라이브러리 미구현

### 3.2 Agent Core (`agent_core.py`)

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| **반복 도구 호출** | ✅ | 최대 10회 (C++: `tool_policy.json`으로 설정 가능) |
| **자동 스킬 인터셉트** | ✅ | `get_device_info` 등 알려진 쿼리 LLM 우회 직접 실행 |
| **멀티 세션** | ✅ | `asyncio.Lock`으로 세션별 히스토리 격리 |
| **스트리밍 응답** | 🔴 | C++: 청크 IPC (`stream_chunk`/`stream_end`) 지원 |
| **컨텍스트 압축** | 🔴 | C++: 15턴 초과 시 LLM 자동 요약 |
| **엣지 메모리 관리** | 🔴 | C++: `malloc_trim(0)` + `sqlite3_release_memory` |
| **백엔드 전환** | 🔴 | C++: `SwitchToBestBackend()` 통합 우선순위 큐 |

### 3.3 도구 인덱서 (`tool_indexer.py`)

| 기능 | 세부사항 |
|------|---------|
| **기본 디렉토리** | `/opt/usr/share/tizenclaw/tools/` |
| **스캔 패턴** | `os.walk()`로 `*.tool.md`, `*.skill.md`, `*.mcp.json` 탐색 |
| **YAML 파서** | 정규식 `^---\n(.*?)\n---` + 라인별 `key: value` 추출 |
| **스키마 출력** | `{name, description, parameters}`, catch-all `arguments` 문자열 |

> **C++ 대비 부족**: `CapabilityRegistry` (FunctionContract 시스템, 21개 내장 capability 등록, 카테고리/부작용/권한 기반 쿼리) 미구현

### 3.4 도구 디스패처 (`tool_dispatcher.py`)

| 도구 타입 | 실행 경로 |
|-----------|----------|
| `cli` | `ContainerEngine.execute_cli_tool()` |
| `skill` | `ContainerEngine.execute_skill()` |
| `mcp` | `ContainerEngine.execute_mcp_tool()` |

> **C++ 대비 부족**: `action_*` 동적 폴백 매칭, `std::shared_mutex` 스레드 안전, `O(1)` 성능 보장 미구현, 단순 Dict 사용

---

## 4. LLM 백엔드 계층

### 추상 인터페이스 (`llm_backend.py`)

| 타입 | 필드 |
|------|------|
| `LlmMessage` | `role`, `text`, `tool_calls`, `tool_call_id` |
| `LlmResponse` | `success`, `text`, `tool_calls`, `prompt_tokens`, `completion_tokens` |
| `LlmToolCall` | `id`, `name`, `args` |
| `LlmToolDecl` | `name`, `description`, `parameters_schema` |

### OpenAI 백엔드 (`openai_backend.py`)

| 기능 | 구현 |
|------|------|
| **HTTP** | `urllib.request` + `asyncio.to_thread` (제로 의존성) |
| **모델** | `gpt-4o` (기본), 생성자에서 설정 가능 |
| **API 키** | 생성자 파라미터 또는 `OPENAI_API_KEY` 환경 변수 |
| **도구 호출** | OpenAI Function Calling (`tools` + `tool_choice: auto`) |
| **타임아웃** | 요청당 30초 |

### C++ 대비 LLM 백엔드 비교

| 기능 | C++ (main/devel) | Python (develPython) |
| --- | --- | --- | 
| **Web Dashboard** | libsoup | `http.server` (asyncio) |
| **Channels** | Telegram, Slack, Discord | Telegram, Slack, Discord, Voice |
| **ActionBridge** | Tizen Action Framework | ctypes FFI to Action Framework |
| **운영 & 터널** | TunnelManager, ActionPolicy | SecureTunnel (Reverse SSH), HealthMonitor |
|------|:---:|:---:|
| 내장 백엔드 수 | **5개** (Gemini, OpenAI, Anthropic, xAI, Ollama) | **1개** (OpenAI 호환) |
| 우선순위 전환 | ✅ 통합 우선순위 큐 | 🔴 단일 백엔드 |
| 자동 폴백 | ✅ 순차 재시도 + 429 백오프 | 🔴 없음 |
| API 키 암호화 | ✅ 디바이스 바인딩 (GLib SHA-256 + XOR) | 🔴 환경 변수만 |
| 스트리밍 | ✅ SSE 청크 파싱 | 🟡 스텁 (단일 응답 래핑) |
| RPK 플러그인 | ✅ 동적 로드 | 🔴 없음 |

---

## 5. 통신 및 IPC

### IPC 프로토콜 (C++와 동일)

| 속성 | 값 |
|------|---|
| **소켓** | Abstract UDS (`\0tizenclaw.sock`) |
| **프레이밍** | `[4바이트 네트워크 엔디안 길이][JSON 페이로드]` |
| **프로토콜** | JSON-RPC 2.0 |
| **동시성** | asyncio `StreamReader`/`StreamWriter` (클라이언트별) |

### CLI 클라이언트 (`tizenclaw_cli.py`)

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 프롬프트 전송 | ✅ | 위치 인자, C++과 동일 |
| `--session` / `-s` | ✅ | 세션 ID 지정 (기본: `cli_test`) |
| `--stream` | ✅ | 스트리밍 모드 (플래그 전달) |
| `--list-agents` | ✅ | 에이전트 목록 조회 |
| `--connect-mcp` | ✅ | MCP 도구 로드 |
| `--list-mcp` | ✅ | MCP 도구 목록 |

> **C++ 대비 부족**: 대화형 모드 (`readline` 루프), `--send-to` 아웃바운드 메시징, 채널 추상화 인터페이스 미구현

---

## 6. 컨테이너 및 스킬 실행

### C++ 대비 비교

| 기능 | C++ | Python |
|------|:---:|:---:|
| **컨테이너 런타임** | crun 1.26 (OCI) | `unshare` 폴백 |
| **격리** | PID/Mount/User 네임스페이스 + seccomp | OS 수준 unshare |
| **프로세스 실행** | `popen()` / `crun exec` | `asyncio.create_subprocess_exec` |
| **IPC** | C++ 길이-프리픽스 UDS | Python 길이-프리픽스 UDS |
| **소켓 활성화** | ✅ systemd `LISTEN_FDS` | ✅ systemd 소켓 유닛 |
| **바인드 마운트** | ✅ `/usr/bin`, `/usr/lib` 등 | 🔴 미구현 |

---

## 7. 데이터 영구 저장

### 세션 저장소 (`session_store.py`)

| 기능 | 구현 |
|------|------|
| **형식** | JSON → Markdown 직렬화 (YAML frontmatter 계획됨) |
| **경로** | `/opt/usr/share/tizenclaw/sessions/{session_id}.md` |
| **원자적 쓰기** | `.tmp` 파일 → `os.replace()` |
| **로깅** | 일별 스킬 실행 로그 (`skills_YYYY-MM-DD.log`, JSON-lines) |

### 메모리 저장소 (`memory_store.py`)

| 타입 | 서브디렉토리 | 보존 기간 | 최대 크기 |
|------|------------|----------|----------|
| **단기** | `short_term/` | 24시간, 최대 50개 | — |
| **장기** | `long_term/` | 영구 | 2KB/파일 |
| **에피소드** | `episodic/` | 30일 | 2KB/파일 |

> **C++ 대비 부족**: 토큰 사용량 추적 (세션별/일별/월별 Markdown 리포트), 감사 로깅 (AuditLogger), `{{MEMORY_CONTEXT}}` 시스템 프롬프트 주입 미구현

---

## 8. RAG 및 시맨틱 검색

### EmbeddingStore (`embedding_store.py`)

| 기능 | C++ | Python |
|------|:---:|:---:|
| SQLite + FTS5 | ✅ | ✅ |
| 벡터 BLOB 저장 | ✅ | ✅ (`struct.pack` floats) |
| 하이브리드 검색 (BM25 + 벡터 RRF) | ✅ | 🟡 (벡터 전용 폴백) |
| 코사인 유사도 | ✅ | ✅ (순수 Python `math`) |
| 토큰 예산 추정 | ✅ | ✅ (`단어 × 1.3`) |
| 텍스트 청킹 | ✅ | ✅ (슬라이딩 윈도우 + 오버랩) |
| 멀티 DB 연결 | ✅ | ✅ (`ATTACH DATABASE`) |
| 임베딩 API (Gemini, OpenAI, Ollama) | ✅ | 🔴 없음 |
| FTS5 자동 동기화 트리거 | ✅ | 🔴 없음 |

### 온디바이스 임베딩 (`on_device_embedding.py`)

| 기능 | C++ | Python |
|------|:---:|:---:|
| ONNX Runtime 세션 | ✅ (dlopen) | 🟡 (로드 가능, 토크나이저 미통합) |
| all-MiniLM-L6-v2 | ✅ (384차원) | 🟡 (제로 벡터 반환) |
| 지연 로드 | ✅ | ✅ |

---

## 9. 워크플로우 엔진

### WorkflowEngine (`workflow_engine.py`)

| 기능 | 세부사항 |
|------|---------|
| **영구 저장** | `/opt/usr/share/tizenclaw/workflows/*.md` |
| **스텝 타입** | `prompt` (LLM), `tool` (ToolDispatcher) |
| **변수 보간** | `{{variable_name}}` |
| **출력 캡처** | `output_var`로 스텝 결과 체이닝 |
| **에러 처리** | 스텝별 `skip_on_failure` |
| **파싱** | YAML frontmatter + `## Step` 헤딩 추출 |

> **C++ 대비 부족**: 조건 분기 (`if/then/else`), 파이프라인 엔진 (별도 PipelineEngine 클래스) 미구현

---

## 10. 태스크 스케줄러

### TaskScheduler (`task_scheduler.py`)

| 기능 | C++ | Python |
|------|:---:|:---:|
| 스케줄 파싱 (daily/interval/once/weekly) | ✅ | 🟡 (타입 정의만, 파서 미완) |
| AgentCore 직접 호출 | ✅ | 🔴 (에이전트 연결 스텁) |
| 재시도 백오프 | ✅ (지수 백오프, 최대 3회) | 🟡 (카운터만) |
| Markdown 영구 저장 | ✅ (YAML frontmatter) | 🔴 미구현 |
| asyncio 동시성 | — | ✅ (2개 asyncio 태스크) |

---

## 11. Tizen 네이티브 연동

### Tizen Dlog 핸들러 (`tizen_dlog.py`)

| 기능 | 세부사항 |
|------|---------|
| **라이브러리** | `libdlog.so.0` via ctypes |
| **로그 태그** | `TIZENCLAW` |
| **우선순위 매핑** | Python `logging` → `DEBUG(3)`, `INFO(4)`, `WARN(5)`, `ERROR(6)` |
| **폴백** | dlog 미발견 시 무동작 (비Tizen 환경) |

### 시스템 이벤트 어댑터 (`tizen_system_event_adapter.py`)

| 기능 | 세부사항 |
|------|---------|
| **라이브러리** | `libcapi-appfw-app-common.so.0` via ctypes |
| **이벤트** | 배터리 충전/레벨, USB 상태, 네트워크 상태 |
| **콜백** | ctypes `CFUNCTYPE` C 콜백 등록 |
| **폴백** | 라이브러리 미발견 시 mock 모드 |

### C++ 대비 Tizen 연동 비교

| 기능 | C++ | Python |
|------|:---:|:---:|
| dlog 로깅 | ✅ | ✅ (ctypes) |
| 시스템 이벤트 어댑터 | ✅ 4개 어댑터 | 🟡 1개 (시스템 이벤트만) |
| Action Framework 브릿지 | ✅ | 🔴 없음 |
| 이벤트 버스 (Pub/Sub) | ✅ | 🔴 없음 |
| 자율 트리거 | ✅ | 🔴 없음 |
| vconf 연동 | ✅ | 🟡 (placeholder) |

---

## 12. 설계 원칙

### 제로 의존성 Python

1. **표준 라이브러리만** — pip 패키지 불필요 (임베딩용 `onnxruntime`/`numpy` 선택사항)
2. **asyncio 우선** — 모든 I/O에 async/await 사용, 협력 스케줄링
3. **ctypes FFI** — 컴파일 없이 Tizen C-API 직접 접근

### C++ 동등 프로토콜

- 동일 IPC 소켓 경로 및 프레이밍 프로토콜
- 동일 도구 스키마 형식 (`.tool.md` YAML frontmatter)
- 동일 CLI 인터페이스 (`tizenclaw-cli` 동일 플래그)
- 동일 systemd 서비스/소켓 구성

### 지연 초기화

- ONNX Runtime은 첫 임베딩 요청 시에만 로드
- Tizen C-API 라이브러리는 사용 가능할 때만 로드
- 작업 디렉토리는 필요 시 생성

---

## 부록: C++ 대비 비교

### 기술 스택

| 구성요소 | C++ (main/devel) | Python (develPython) |
|---------|:---:|:---:|
| **언어** | C++20 | Python 3.x |
| **빌드** | CMake 3.12+ (컴파일) | CMake (`LANGUAGES NONE`, 설치만) |
| **HTTP** | libcurl (클라이언트), libsoup (서버) | `urllib.request` |
| **WebSocket** | libwebsockets | 없음 |
| **DB** | SQLite3 | SQLite3 |
| **ML** | ONNX Runtime (dlopen) | ONNX Runtime (선택사항) |
| **JSON** | nlohmann/json | stdlib `json` |
| **동시성** | `std::thread` + `std::mutex` | `asyncio` |
| **테스트** | Google Test (42개 파일) | Shell 검증 (28개 스크립트) |

### 미포팅 C++ 기능 (주요 갭)

| 카테고리 | 미포팅 기능 | 영향도 |
|---------|-----------|:---:|
| **LLM** | Gemini/Anthropic/xAI/Ollama 백엔드 | 높음 |
| **LLM** | 통합 우선순위 전환 및 자동 폴백 | 높음 |
| **LLM** | API 키 암호화 (디바이스 바인딩) | 높음 |
| **채널** | Telegram/Slack/Discord/Webhook/Voice | 높음 |
| **채널** | 아웃바운드 메시징 및 브로드캐스트 | 중간 |
| **보안** | UID 인증 (`SO_PEERCRED`) | 높음 |
| **보안** | 도구 실행 정책 (ToolPolicy) | 높음 |
| **보안** | 루프 감지 (3회 반복 차단) | 중간 |
| **보안** | 감사 로깅 (AuditLogger) | 중간 |
| **에이전트** | 멀티 에이전트 시스템 (11-Agent MVP) | 높음 |
| **에이전트** | 슈퍼바이저 엔진 | 높음 |
| **에이전트** | A2A 프로토콜 (크로스 디바이스) | 중간 |
| **인프라** | 이벤트 버스 (Pub/Sub) | 중간 |
| **인프라** | 자율 트리거 엔진 | 중간 |
| **인프라** | 스킬 핫리로드 (inotify) | 중간 |
| **인프라** | OTA 업데이터 | 낮음 |
| **인프라** | Fleet 관리 | 낮음 |
| **인프라** | 보안 터널링 (ngrok) | 낮음 |
| **플러그인** | RPK/TPK 플러그인 매니저 | 중간 |
| **데이터** | Capability Registry (FunctionContract) | 중간 |
| **지식** | 하이브리드 RAG (BM25 + 벡터 RRF 완전 구현) | 중간 |
| **지식** | 온디바이스 OCR (PaddleOCR) | 낮음 |

### 모듈 인벤토리

| 모듈 | 파일 | LOC | 설명 |
|------|------|----:|------|
| 데몬 | `tizenclaw_daemon.py` | 179 | IPC 서버 + MCP stdio |
| CLI | `tizenclaw_cli.py` | 101 | CLI 클라이언트 |
| 도구 실행기 | `tizenclaw_tool_executor.py` | 62 | 소켓 활성화 도구 실행 |
| 코드 샌드박스 | `tizenclaw_code_sandbox.py` | 19 | 스텁 샌드박스 |
| AgentCore | `core/agent_core.py` | 143 | Agentic Loop 오케스트레이션 |
| ToolIndexer | `core/tool_indexer.py` | 119 | 도구 스키마 발견 |
| ToolDispatcher | `core/tool_dispatcher.py` | 44 | 도구 라우팅 |
| WorkflowEngine | `core/workflow_engine.py` | 162 | 파이프라인 실행 |
| LlmBackend | `llm/llm_backend.py` | 70 | 추상 LLM 인터페이스 |
| OpenAiBackend | `llm/openai_backend.py` | 99 | OpenAI REST 클라이언트 |
| ContainerEngine | `infra/container_engine.py` | 66 | 도구 실행기 IPC |
| EventAdapter | `infra/tizen_system_event_adapter.py` | 98 | ctypes 이벤트 핸들러 |
| SessionStore | `storage/session_store.py` | 81 | 세션 영구 저장 |
| MemoryStore | `storage/memory_store.py` | 117 | 3단계 메모리 시스템 |
| EmbeddingStore | `storage/embedding_store.py` | 152 | SQLite RAG 저장소 |
| OnDeviceEmbedding | `embedding/on_device_embedding.py` | 69 | ONNX 추론 |
| TaskScheduler | `scheduler/task_scheduler.py` | 108 | 스케줄러 |
| TizenDlog | `utils/tizen_dlog.py` | 58 | dlog 로깅 |
| NativeWrapper | `utils/native_wrapper.py` | 26 | ctypes 바인딩 |
| **합계** | | **~1,773** | |
