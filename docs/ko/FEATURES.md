# TizenClaw 기능 매트릭스

> **최종 업데이트**: 2026-03-22

이 문서는 TizenClaw의 모든 기능을 카테고리별로 정리하여 현재 구현 상태를 제공합니다.

---

## 범례

| 기호 | 의미 |
|:----:|------|
| ✅ | 완전히 구현 및 검증 완료 |
| 🟡 | 부분 구현 / 스텁 |
| 🔴 | 미구현 / 계획 중 |

---

## 1. 핵심 에이전트 시스템

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| Agentic Loop (반복 도구 호출) | ✅ | `tool_policy.json`으로 `max_iterations` 설정 가능 |
| LLM 스트리밍 응답 | ✅ | 청크 IPC 전달 (`stream_chunk` / `stream_end`) |
| 컨텍스트 압축 | ✅ | LLM 기반 요약 (가장 오래된 10턴 → 1개 압축) |
| 멀티 세션 지원 | ✅ | 세션별 시스템 프롬프트 및 히스토리 격리 |
| 엣지 메모리 관리 | ✅ | 5분 유휴 시 `malloc_trim(0)` + `sqlite3_release_memory` |
| JSON-RPC 2.0 IPC | ✅ | Abstract Unix Domain Socket 위 길이-프리픽스 프레이밍 |
| 동시 클라이언트 처리 | ✅ | 스레드 풀, `kMaxConcurrentClients = 4` |
| UID 인증 | ✅ | `SO_PEERCRED` (root, app_fw, system, developer) |
| 시스템 프롬프트 외부화 | ✅ | 4단계 fallback (config → 파일 → 기본 → 하드코딩) |
| 동적 도구 주입 | ✅ | `{{AVAILABLE_TOOLS}}`, `{{CAPABILITY_SUMMARY}}` placeholder |
| 병렬 도구 실행 | ✅ | `std::async`를 통한 동시 도구 호출 |

## 2. LLM 백엔드

| 백엔드 | 상태 | 기본 모델 | 스트리밍 | 토큰 카운팅 |
|--------|:----:|:---:|:---------:|:--------------:|
| Google Gemini | ✅ | `gemini-2.5-flash` | ✅ | ✅ |
| OpenAI | ✅ | `gpt-4o` | ✅ | ✅ |
| Anthropic (Claude) | ✅ | `claude-sonnet-4-20250514` | ✅ | ✅ |
| xAI (Grok) | ✅ | `grok-3` | ✅ | ✅ |
| Ollama (로컬) | ✅ | `llama3` | ✅ | ✅ |
| RPK 플러그인 백엔드 | ✅ | 커스텀 | ✅ | ✅ |

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 통합 우선순위 전환 | ✅ | Plugin > active > fallback, 설정 가능한 우선순위 |
| 자동 폴백 | ✅ | 순차 재시도, 429 rate-limit 백오프 |
| API 키 암호화 | ✅ | 디바이스 바인딩 `ENC:` 프리픽스 (하위 호환) |
| 세션별 사용량 추적 | ✅ | 세션별/일별/월별 Markdown 리포트 |

## 3. 통신 채널

| 채널 | 상태 | 프로토콜 | 아웃바운드 | 라이브러리 |
|------|:----:|---------|:--------:|---------|
| Telegram | ✅ | Bot API Long-Polling | ✅ | libcurl |
| Slack | ✅ | Socket Mode (WebSocket) | ✅ | libwebsockets |
| Discord | ✅ | Gateway WebSocket | ✅ | libwebsockets |
| MCP (Claude Desktop) | ✅ | stdio JSON-RPC 2.0 | ❌ | 내장 |
| Webhook | ✅ | HTTP 인바운드 (libsoup) | ❌ | libsoup |
| Voice (STT/TTS) | ✅ | Tizen STT/TTS C-API | ✅ | 조건부 컴파일 |
| Web Dashboard | ✅ | libsoup SPA (port 9090) | ❌ | libsoup |
| SO 플러그인 | ✅ | C API (`tizenclaw_channel.h`) | 선택 | dlopen |

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 채널 추상화 인터페이스 | ✅ | C++ `Channel` 베이스 클래스 |
| 설정 기반 활성화 | ✅ | `channels.json` 활성/비활성 |
| 아웃바운드 메시징 | ✅ | `SendTo(channel, text)` + `Broadcast(text)` |
| 채널 allowlist | ✅ | 채널별 chat_id/guild 허용 목록 |
| 웨이크 워드 감지 | 🔴 | 하드웨어 마이크 지원 필요 |
| WhatsApp 채널 | 🔴 | 미구현 |
| 이메일 채널 | 🔴 | 미구현 |

## 4. 스킬 및 도구 생태계

### 4.1 네이티브 CLI 도구 스위트 (13개 디렉토리)

| 카테고리 | 도구 | 상태 | C-API | 비동기 |
|----------|------|:----:|-------|:-----:|
| **앱 관리** | `list_apps` | ✅ | `app_manager` | |
| | `send_app_control` | ✅ | `app_control` | |
| | `terminate_app` | ✅ | `app_manager` | |
| | `get_package_info` | ✅ | `package_manager` | |
| **디바이스 정보** | `get_device_info` | ✅ | `system_info` | |
| | `get_system_info` | ✅ | `system_info` | |
| | `get_runtime_info` | ✅ | `runtime_info` | |
| | `get_storage_info` | ✅ | `storage` | |
| | `get_sensor_data` | ✅ | `sensor` | |
| | `get_thermal_info` | ✅ | `device` | |
| **네트워크** | `get_wifi_info` | ✅ | `wifi-manager` | |
| | `get_bluetooth_info` | ✅ | `bluetooth` | |
| | `get_network_info` | ✅ | `connection` | |
| | `scan_wifi_networks` | ✅ | `wifi-manager` | ⚡ |
| | `scan_bluetooth_devices` | ✅ | `bluetooth` | ⚡ |
| **디스플레이 & HW** | `get_display_info` | ✅ | `device` | |
| | `control_display` | ✅ | `device` | |
| | `control_haptic` | ✅ | `device` | |
| | `control_led` | ✅ | `device` | |
| | `control_volume` | ✅ | `sound_manager` | |
| | `control_power` | ✅ | `device` | |
| **미디어** | `get_battery_info` | ✅ | `device` | |
| | `get_sound_devices` | ✅ | `sound_manager` | |
| | `get_media_content` | ✅ | `media-content` | |
| | `get_metadata` | ✅ | `metadata-extractor` | |
| **시스템** | `send_notification` | ✅ | `notification` | |
| | `schedule_alarm` | ✅ | `alarm` | |
| | `download_file` | ✅ | `url-download` | ⚡ |
| | `web_search` | ✅ | Wikipedia API | |

> ⚡ = tizen-core 이벤트 루프를 사용하는 비동기 스킬

### 4.2 내장 도구 (AgentCore, 네이티브 C++)

| 도구 | 상태 | 카테고리 |
|------|:----:|---------|
| `execute_code` | ✅ | 코드 실행 |
| `manage_custom_skill` | ✅ | 스킬 관리 |
| `create_task` / `list_tasks` / `cancel_task` | ✅ | 태스크 스케줄러 |
| `create_session` / `list_sessions` / `send_to_session` | ✅ | 멀티 에이전트 |
| `run_supervisor` | ✅ | 멀티 에이전트 |
| `ingest_document` / `search_knowledge` | ✅ | RAG |
| `execute_action` / `action_<name>` | ✅ | Tizen Action Framework |
| `execute_cli` | ✅ | CLI 도구 플러그인 |
| `create_workflow` / `run_workflow` 등 | ✅ | 워크플로우 엔진 |
| `create_pipeline` / `run_pipeline` 등 | ✅ | 파이프라인 엔진 |
| `remember` / `recall` / `forget` | ✅ | 영속 메모리 |

### 4.3 확장성

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| RPK 스킬 플러그인 | ✅ | 플랫폼 서명 RPK를 통한 Python 스킬 |
| CLI 도구 플러그인 (TPK) | ✅ | `.tool.md` 설명서를 가진 네이티브 바이너리 |
| LLM 백엔드 플러그인 (RPK) | ✅ | 우선순위를 가진 커스텀 LLM 백엔드 |
| 채널 플러그인 (.so) | ✅ | C API를 통한 공유 라이브러리 플러그인 |
| 스킬 핫리로드 (inotify) | ✅ | 재시작 없이 신규/수정 스킬 자동 감지 |
| Capability Registry | ✅ | 함수 계약을 가진 통합 도구 등록 |
| SKILL.md 형식 | ✅ | Anthropic 표준 스킬 형식 |
| 원격 스킬 마켓플레이스 | 🟡 | REST API 스텁, 대시보드 UI 미지원 |
| Per-Skill seccomp | 🔴 | 모든 스킬이 컨테이너 보안 프로파일 공유 |
| Per-Skill 리소스 쿼터 | 🔴 | 실행당 CPU/메모리 제한 없음 |

## 5. 보안

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| OCI 컨테이너 격리 | ✅ | crun + PID/Mount/User 네임스페이스 |
| 도구 실행 정책 | ✅ | 위험도 (low/medium/high), 차단 스킬 목록 |
| 루프 감지 | ✅ | 동일 도구+인자 3회 → 차단 + idle 진행 체크 |
| API 키 암호화 | ✅ | 디바이스 바인딩 GLib SHA-256 + XOR |
| 감사 로깅 | ✅ | 일별 Markdown 테이블, 5MB 로테이션 |
| UID 인증 | ✅ | `SO_PEERCRED` IPC 소켓 |
| 관리자 인증 | ✅ | 세션 토큰 + SHA-256 비밀번호 해싱 |
| Webhook HMAC | ✅ | HMAC-SHA256 서명 검증 |
| 플랫폼 인증서 서명 | ✅ | RPK/TPK 플러그인 설치 시 필수 |
| 스킬별 네트워크 접근 제어 | 🔴 | 스킬별 네트워크 허용/거부 없음 |

## 6. 지식 및 인텔리전스

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 하이브리드 RAG 검색 | ✅ | BM25 키워드(FTS5) + 벡터 코사인, RRF(k=60) |
| 온디바이스 임베딩 | ✅ | ONNX Runtime `all-MiniLM-L6-v2` (384차원, 지연 로드) |
| 멀티 DB 지원 | ✅ | 다중 지식 데이터베이스 동시 연결 |
| 영속 메모리 | ✅ | 장기/에피소드/단기 + LLM 도구 |
| 메모리 요약 | ✅ | idle 시 자동 갱신 `memory.md` |
| 온디바이스 OCR | ✅ | PaddleOCR PP-OCRv3 |
| ANN 인덱스 (HNSW) | 🔴 | 현재 순차 코사인 유사도 |

## 7. 자동화 및 오케스트레이션

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 태스크 스케줄러 | ✅ | Cron/interval/once/weekly + 재시도 백오프 |
| 슈퍼바이저 에이전트 | ✅ | 목표 분해 → 위임 → 검증 |
| 스킬 파이프라인 | ✅ | `{{variable}}` 보간을 가진 순차 실행 |
| 조건 분기 | ✅ | 파이프라인 `if/then/else` |
| 워크플로우 엔진 | ✅ | 내장 도구를 통한 CRUD + 실행 |
| 자율 트리거 | ✅ | LLM 평가를 가진 이벤트 기반 규칙 |
| A2A 프로토콜 | ✅ | 크로스 디바이스 HTTP JSON-RPC 2.0 |
| 이벤트 버스 | ✅ | 시스템 이벤트 Pub/Sub |
| 병렬 태스크 실행 | 🔴 | 현재 순차, 의존성 그래프 계획 |

## 8. 운영 및 배포

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| systemd 서비스 | ✅ | `tizenclaw.service` (Type=simple) |
| 소켓 활성화 | ✅ | Tool executor + code sandbox 온디맨드 |
| GBS RPM 패키징 | ✅ | x86_64, armv7l, aarch64 |
| 자동 배포 | ✅ | `deploy.sh` (빌드 + 설치 + 재시작) |
| 웹 대시보드 | ✅ | 글래스모피즘 SPA, 포트 9090 |
| 헬스 메트릭 | ✅ | Prometheus 스타일 `/api/metrics` |
| OTA 업데이트 | ✅ | HTTP 풀, 버전 확인, 롤백 |
| Fleet 관리 | 🟡 | 등록 및 heartbeat (스텁) |
| 보안 터널링 | ✅ | ngrok |
| 설정 편집기 | ✅ | 7개+ 설정 파일 인브라우저 편집 |
| C-API SDK 라이브러리 | 🟡 | `libtizenclaw` 구현, 배포 미완 |

## 9. MCP (Model Context Protocol)

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| MCP 서버 (내장) | ✅ | C++ stdio JSON-RPC 2.0 (Claude Desktop용) |
| MCP 클라이언트 (내장) | ✅ | 외부 MCP 도구 서버 연결 |
| MCP 샌드박스 | ✅ | MCP 서버가 보안 컨테이너 내부에서 실행 |
| MCP를 통한 도구 노출 | ✅ | 모든 등록 도구를 MCP로 제공 |

## 10. 테스트

| 기능 | 상태 | 세부사항 |
|------|:----:|---------|
| 유닛 테스트 (gtest/gmock) | ✅ | 42개 파일 (~7,800 LOC, 205+ 테스트) |
| E2E 스모크 테스트 | ✅ | 2개 스크립트 |
| CLI 도구 검증 | ✅ | `tests/verification/cli_tools/` |
| MCP 적합성 테스트 | ✅ | `tests/verification/mcp/` |
| 빌드 시 테스트 | ✅ | RPM `%check`에서 `ctest -V` |
| LLM 통합 테스트 | ✅ | `tests/verification/llm_integration/` |

---

## 참고 문서

- [설계 문서](DESIGN.md) — 전체 아키텍처 및 모듈 설명
- [도구 레퍼런스](TOOLS.md) — 스킬/도구 카탈로그
- [ML/AI 에셋](ASSETS.md) — ONNX Runtime, RAG 데이터베이스, OCR
- [C-API 가이드](API_GUIDE.md) — SDK 사용법 및 코드 예제
