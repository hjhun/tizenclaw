# TizenClaw 기능 매트릭스 — Python 포팅

> **최종 업데이트**: 2026-03-23
> **브랜치**: `develPython`

이 문서는 TizenClaw Python 포팅의 모든 기능을 C++ 버전과 비교하여 현재 구현 상태를 제공합니다.

---

## 범례

| 기호 | 의미 |
|:----:|------|
| ✅ | 완전히 구현 및 검증 완료 |
| 🟡 | 부분 구현 / 스텁 |
| 🔴 | 미구현 / 계획 중 |
| ➖ | Python 포팅 대상 아님 |

---

## 1. 핵심 에이전트 시스템

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| Agentic Loop (반복 도구 호출) | ✅ | ✅ | `AgentCore.process_prompt()`에서 최대 10회 반복 |
| LLM 스트리밍 응답 | ✅ | 🟡 | 단일 응답 래핑 스텁 (`generate_stream`) |
| 컨텍스트 압축 | ✅ | 🔴 | C++: 15턴 초과 시 LLM 자동 요약 |
| 멀티 세션 지원 | ✅ | ✅ | `asyncio.Lock`으로 세션별 히스토리 격리 |
| 엣지 메모리 관리 | ✅ | 🔴 | C++: 5분 유휴 시 `malloc_trim(0)` + SQLite 캐시 해제 |
| JSON-RPC 2.0 IPC | ✅ | ✅ | 동일 프로토콜, 동일 프레이밍 (`[4B 길이][JSON]`) |
| 동시 클라이언트 처리 | ✅ | ✅ | asyncio 협력 동시성 (C++: 4-클라이언트 스레드 풀) |
| UID 인증 | ✅ | 🔴 | C++: `SO_PEERCRED` 검증 |
| 시스템 프롬프트 외부화 | ✅ | 🔴 | C++: 4단계 fallback (config→파일→기본→하드코딩) |
| 동적 도구 주입 | ✅ | ✅ | `ToolIndexer.get_tool_schemas()`가 LLM에 제공 |
| 자동 스킬 인터셉트 | ✅ | ✅ | `get_device_info` 등 LLM 우회 직접 실행 |
| 병렬 도구 실행 | ✅ | 🔴 | C++: `std::async` 동시 도구 호출 |

## 2. LLM 백엔드

| 백엔드 | C++ | Python | 기본 모델 | 스트리밍 | 토큰 카운팅 |
|--------|:---:|:------:|:---:|:---------:|:-----------:|
| Google Gemini | ✅ | 🔴 | — | — | — |
| OpenAI | ✅ | ✅ | `gpt-4o` | 🟡 | 🔴 |
| Anthropic (Claude) | ✅ | 🔴 | — | — | — |
| xAI (Grok) | ✅ | 🔴 | — | — | — |
| Ollama (로컬) | ✅ | 🔴 | — | — | — |
| RPK 플러그인 | ✅ | 🔴 | — | — | — |

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| 통합 우선순위 전환 | ✅ | 🔴 | 단일 백엔드만 사용 |
| 자동 폴백 | ✅ | 🔴 | 폴백 체인 없음 |
| API 키 암호화 | ✅ | 🔴 | 환경 변수만 사용 |
| 세션별 사용량 추적 | ✅ | 🔴 | 미구현 |
| 제로 외부 의존성 | 🔴 | ✅ | stdlib `urllib.request` + `asyncio.to_thread` |

## 3. 통신 채널

| 채널 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| Telegram | ✅ | 🔴 | 미포팅 |
| Slack | ✅ | 🔴 | 미포팅 |
| Discord | ✅ | 🔴 | 미포팅 |
| MCP (Claude Desktop) | ✅ | ✅ | `--mcp-stdio` 모드 |
| Webhook | ✅ | 🔴 | 미포팅 |
| Voice (STT/TTS) | ✅ | 🔴 | 미포팅 |
| Web Dashboard | ✅ | ✅ | C++의 정적 파일 유지 |
| SO 플러그인 | ✅ | ➖ | dlopen 해당 없음 |

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| 채널 추상화 인터페이스 | ✅ | 🔴 | ChannelRegistry 없음 |
| tizenclaw-cli | ✅ | ✅ | 동일 플래그 (`-s`, `--stream`, `--list-agents` 등) |
| IPC 클라이언트 라이브러리 | ✅ | ✅ | `SocketClient` 클래스 |

## 4. 스킬 및 도구 생태계

### 4.1 네이티브 CLI 도구 스위트 (13개 디렉토리)

| 카테고리 | 도구 수 | C++ | Python | 비고 |
|----------|:-----:|:---:|:------:|------|
| 앱 관리 | 4 | ✅ | ✅ | 동일 CLI 도구, 도구 실행기 경유 |
| 디바이스 정보 | 7 | ✅ | ✅ | 동일 CLI 도구, ctypes FFI |
| 네트워크 | 6 | ✅ | ✅ | 동일 CLI 도구 |
| 디스플레이 & HW | 6 | ✅ | ✅ | 동일 CLI 도구 |
| 미디어 | 5 | ✅ | ✅ | 동일 CLI 도구 |
| 시스템 | 6 | ✅ | ✅ | 동일 CLI 도구 |

> CLI 도구는 C++과 Python 버전 간 **공유**됩니다 (독립 실행 파일).

### 4.2 도구 발견 및 디스패치

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| ToolIndexer (스키마 스캔) | ✅ | ✅ | 정규식 YAML frontmatter 파서 |
| ToolDispatcher (라우팅) | ✅ | ✅ | `cli` / `skill` / `mcp` 타입 라우팅 |
| Capability Registry | ✅ | 🔴 | FunctionContract 시스템 없음 |
| O(1) 도구 조회 | ✅ | ✅ | `Dict[str, Dict]` 해시맵 |
| `.tool.md` 형식 | ✅ | ✅ | 동일 형식, 동일 파서 |
| `.skill.md` 형식 | ✅ | ✅ | 동일 형식 |

### 4.3 내장 도구 스키마 (17개 파일)

| 도구 | C++ | Python |
|------|:---:|:------:|
| `execute_code` | ✅ | ✅ |
| `create_task` / `list_tasks` / `cancel_task` | ✅ | ✅ |
| `create_session` | ✅ | ✅ |
| `ingest_document` / `search_knowledge` | ✅ | ✅ |
| `create/list/run/delete_workflow` | ✅ | ✅ |
| `create/list/run/delete_pipeline` | ✅ | ✅ |
| `run_supervisor` | ✅ | ✅ |
| `generate_web_app` | ✅ | ✅ |

### 4.4 확장성

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| RPK 스킬 플러그인 | ✅ | 🔴 | SkillPluginManager 미포팅 |
| CLI 도구 플러그인 (TPK) | ✅ | 🔴 | CliPluginManager 미포팅 |
| LLM 백엔드 플러그인 | ✅ | 🔴 | PluginManager 미포팅 |
| 채널 플러그인 (.so) | ✅ | ➖ | 해당 없음 |
| 스킬 핫리로드 (inotify) | ✅ | 🔴 | 파일 감시자 없음 |
| SKILL.md 형식 | ✅ | ✅ | 표준 형식, ToolIndexer 파싱 |

## 5. 보안

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| OCI 컨테이너 격리 | ✅ | 🟡 | crun 대신 `unshare` 폴백 |
| 도구 실행 정책 | ✅ | 🔴 | ToolPolicy 클래스 없음 |
| 루프 감지 | ✅ | 🔴 | 반복 감지 없음 |
| API 키 암호화 | ✅ | 🔴 | 환경 변수만 |
| 감사 로깅 | ✅ | 🔴 | AuditLogger 없음 |
| UID 인증 | ✅ | 🔴 | SO_PEERCRED 없음 |
| 관리자 인증 | ✅ | 🔴 | 웹 인증 없음 |
| 페이로드 크기 보호 | 🟡 | ✅ | 10MB 제한 |

## 6. 지식 및 인텔리전스

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| 하이브리드 RAG 검색 | ✅ | 🟡 | Placeholder (벡터 전용 폴백) |
| 온디바이스 임베딩 | ✅ | 🟡 | ONNX 세션 로드 가능, 토크나이저 미통합 → 제로 벡터 |
| SQLite FTS5 | ✅ | ✅ | FTS5 가상 테이블 생성 |
| 멀티 DB 지원 | ✅ | ✅ | `ATTACH DATABASE` 구현 |
| 토큰 예산 추정 | ✅ | ✅ | `단어 × 1.3` |
| 코사인 유사도 | ✅ | ✅ | 순수 Python math 구현 |
| 텍스트 청킹 | ✅ | ✅ | 슬라이딩 윈도우 + 오버랩 |
| 영속 메모리 | ✅ | ✅ | 장기/에피소드/단기 |
| 메모리 요약 | ✅ | 🟡 | 스텁 `regenerate_summary()` |

## 7. 자동화 및 오케스트레이션

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| 태스크 스케줄러 | ✅ | ✅ | asyncio 기반 (cron/interval/once/weekly) |
| 워크플로우 엔진 | ✅ | ✅ | Markdown 파싱 + 변수 보간 |
| 변수 보간 | ✅ | ✅ | `{{variable}}` |
| 조건 분기 | ✅ | 🔴 | 워크플로우 파서에 미구현 |
| 슈퍼바이저 에이전트 | ✅ | 🔴 | SupervisorEngine 없음 |
| 스킬 파이프라인 | ✅ | 🟡 | WorkflowEngine 스텝으로 대체 |
| 자율 트리거 | ✅ | 🔴 | AutonomousTrigger 없음 |
| 이벤트 버스 | ✅ | 🔴 | Pub/Sub 시스템 없음 |
| A2A 프로토콜 | ✅ | 🔴 | 크로스 디바이스 프로토콜 없음 |

## 8. 운영 및 배포

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| systemd 서비스 | ✅ | ✅ | Python 스크립트 실행 |
| 소켓 활성화 | ✅ | ✅ | Tool executor + code sandbox 소켓 |
| GBS RPM 패키징 | ✅ | ✅ | 설치 전용 CMake (`LANGUAGES NONE`) |
| 자동 배포 | ✅ | ✅ | `deploy.sh` 스크립트 |
| 웹 대시보드 | ✅ | ✅ | 정적 파일 (5개, ~3,900 LOC) |
| 헬스 메트릭 | ✅ | 🔴 | `/api/metrics` 없음 |
| OTA 업데이트 | ✅ | 🔴 | OtaUpdater 없음 |
| Fleet 관리 | 🟡 | 🔴 | 미포팅 |
| 보안 터널링 | ✅ | 🔴 | TunnelManager 없음 |
| 디버그 서비스 | ✅ | ✅ | `tizenclaw-debug.service` |

## 9. MCP (Model Context Protocol)

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| MCP 서버 (내장) | ✅ | ✅ | `--mcp-stdio` 모드 |
| MCP 클라이언트 (내장) | ✅ | 🔴 | McpClientManager 없음 |
| MCP 샌드박스 | ✅ | 🔴 | 컨테이너 기반 MCP 서버 없음 |
| MCP 통한 도구 노출 | ✅ | ✅ | 모든 ToolIndexer 스키마 제공 |

## 10. 테스트

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| 유닛 테스트 (gtest) | ✅ | ➖ | 레거시 C++ 파일 유지, 미컴파일 |
| Shell 검증 테스트 | ✅ | ✅ | `tests/verification/` (28개 스크립트) |
| E2E 스모크 테스트 | ✅ | ✅ | `tests/e2e/` |
| CLI 도구 검증 | ✅ | ✅ | `tests/verification/cli_tools/` (13개) |
| MCP 적합성 테스트 | ✅ | ✅ | `tests/verification/mcp/` (2개) |
| LLM 통합 테스트 | ✅ | ✅ | `tests/verification/llm_integration/` (3개) |
| 회귀 테스트 | ✅ | ✅ | `tests/verification/regression/` |
| Python 유닛 테스트 (pytest) | ➖ | 🔴 | 미작성 |

## 11. Tizen 네이티브 연동

| 기능 | C++ | Python | 세부사항 |
|------|:---:|:------:|---------|
| Tizen dlog 라우팅 | ✅ | ✅ | `ctypes` → `libdlog.so.0` |
| 시스템 이벤트 핸들러 | ✅ | ✅ | `ctypes` → `libcapi-appfw-app-common.so.0` |
| vconf 연동 | ✅ | 🟡 | NativeWrapper에 Placeholder |
| Action Framework | ✅ | 🔴 | ActionBridge 없음 |

---

## 포팅 커버리지 요약

| 카테고리 | C++ 기능 수 | Python 포팅 | 커버리지 |
|---------|:---:|:---:|:---:|
| 핵심 에이전트 | 11 | 6 | **55%** |
| LLM 백엔드 | 6 + 5 기능 | 1 + 1 기능 | **~18%** |
| 통신 채널 | 8 | 2 (CLI + MCP) | **25%** |
| 도구 및 스킬 | 13 CLI + 17 내장 | 13 CLI + 17 내장 | **100%** |
| 보안 | 8 | 1 | **13%** |
| 지식 | 8 | 5 | **63%** |
| 자동화 | 9 | 2 | **22%** |
| 운영 | 10 | 5 | **50%** |
| MCP | 4 | 2 | **50%** |
| 테스트 | 7 | 5 | **71%** |

### 핵심 요약

> **Python 포팅은 핵심 에이전트 기능(Agentic Loop, 도구 디스패치, CLI 동등성)과 동일한 도구 생태계를 제공합니다.** 멀티 LLM 백엔드, 채널, 보안, 고급 자동화 등의 영역은 추가 포팅이 필요합니다.

---

## 참고 문서

- [설계 문서](DESIGN.md) — 전체 아키텍처 및 모듈 설명
- [도구 레퍼런스](TOOLS.md) — 스킬/도구 카탈로그
- [ML/AI 에셋](ASSETS.md) — ONNX Runtime, RAG 데이터베이스, OCR
