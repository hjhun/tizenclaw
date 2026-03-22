# TizenClaw 도구 레퍼런스

> **최종 업데이트**: 2026-03-22

TizenClaw는 **13개 네이티브 CLI 도구 스위트** (C++ 커맨드라인 실행파일), **20개+ 내장 도구** (네이티브 C++), RPK/TPK 플러그인을 통한 동적 도구 확장을 지원합니다. 모든 도구는 함수 계약을 가진 [Capability Registry](DESIGN.md#34-capability-registry)에 등록됩니다.

> **Anthropic 표준 호환**: TizenClaw의 스킬 시스템은 **Anthropic 표준 스킬 포맷** (`SKILL.md`, YAML frontmatter, JSON 스키마 파라미터)을 완전히 구현합니다. 또한 내장 **MCP 클라이언트**로 외부 MCP 도구 서버에 접속할 수 있습니다.

> CLI 도구 스위트는 `ctypes` FFI를 사용하여 Tizen C-API를 직접 호출합니다. 비동기 스킬(⚡)은 **tizen-core** 이벤트 루프를 사용합니다.

---

## 네이티브 CLI 도구 스위트 (13개 디렉토리)

### 1. 앱 관리 (`tizen-app-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `list_apps`, `send_app_control`, `terminate_app`, `get_package_info` |
| `--app-id` | string | 대상 앱 ID (선택) |
| `--operation` | string | 앱 컨트롤 작업 URI (선택) |

**C-API**: `app_manager`, `app_control`, `package_manager`

### 2. 디바이스 정보 (`tizen-device-info-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `get_device_info`, `get_system_info`, `get_runtime_info`, `get_storage_info`, `get_system_settings`, `get_sensor_data`, `get_thermal_info` |
| `--sensor-type` | string | 센서 타입 (선택) |

**C-API**: `system_info`, `runtime_info`, `storage`, `system_settings`, `sensor`, `device`

### 3. 네트워크 (`tizen-network-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `get_wifi_info`, `scan_wifi_networks` ⚡, `get_bluetooth_info`, `scan_bluetooth_devices` ⚡, `get_network_info`, `get_data_usage` |

**C-API**: `wifi-manager`, `bluetooth`, `connection`

### 4. 디스플레이 및 하드웨어 (`tizen-display-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `get_display_info`, `control_display`, `control_haptic`, `control_led`, `control_volume`, `control_power` |
| `--brightness` | int | 밝기 값 (선택) |
| `--volume` | int | 볼륨 값 (선택) |

**C-API**: `device` (display, haptic, flash, power), `sound_manager`

### 5. 미디어 (`tizen-media-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `get_battery_info`, `get_sound_devices`, `get_media_content`, `get_metadata`, `get_mime_type` |

**C-API**: `device` (battery), `sound_manager`, `media-content`, `metadata-extractor`, `mime-type`

### 6. 시스템 액션 (`tizen-system-cli`)

| 파라미터 | 타입 | 설명 |
|---------|------|------|
| `--command` | string | `play_tone`, `play_feedback`, `send_notification`, `schedule_alarm`, `download_file` ⚡, `web_search` |
| `--url` | string | 다운로드 URL (선택) |
| `--message` | string | 알림 내용 (선택) |

**C-API**: `tone_player`, `feedback`, `notification`, `alarm`, `url-download`

> ⚡ = tizen-core 이벤트 루프를 사용하는 비동기 스킬

---

## 내장 도구 (AgentCore에서 네이티브 C++ 구현)

| 도구 | 설명 |
|------|------|
| `execute_code` | 샌드박스된 Python 코드 실행 |
| `manage_custom_skill` | 커스텀 스킬 CRUD |
| `create_task` | 예약 태스크 생성 (daily/interval/once/weekly) |
| `list_tasks` | 활성 태스크 목록 |
| `cancel_task` | 태스크 취소 |
| `create_session` | 에이전트 세션 생성 |
| `list_sessions` | 활성 세션 목록 |
| `send_to_session` | 특정 세션에 프롬프트 전송 |
| `run_supervisor` | 슈퍼바이저 에이전트 실행 (목표 분해/위임) |
| `ingest_document` | RAG 문서 수집 (청킹 + 임베딩) |
| `search_knowledge` | RAG 시맨틱 검색 |
| `execute_action` | Tizen Action Framework 액션 실행 |
| `action_<name>` | Per-Action 타입 도구 (디바이스별) |
| `execute_cli` | CLI 도구 플러그인 실행 |
| `create_workflow` / `run_workflow` | 워크플로우 CRUD + 실행 |
| `create_pipeline` / `run_pipeline` | 파이프라인 CRUD + 실행 |
| `remember` | 장기/에피소드 메모리 저장 |
| `recall` | 키워드 메모리 검색 |
| `forget` | 특정 메모리 삭제 |

---

## RPK 도구 배포 및 확장성

### RPK 스킬 플러그인

RPK (Resource Package)를 통해 Python 스킬을 데몬 재컴파일 없이 동적으로 배포:

```
skills/
├── weather_check/          ← RPK에서 배포된 스킬
│   ├── SKILL.md            ← Anthropic 표준 형식
│   ├── main.py             ← 진입점
│   └── requirements.txt    ← Python 의존성
```

- 플랫폼 레벨 인증서 서명 필수
- `SkillPluginManager`가 RPK `lib/<skill_name>/`에서 자동 symlink
- inotify 핫리로드로 즉시 사용 가능

### CLI 도구 플러그인 (TPK)

TPK (Tizen Package)를 통해 네이티브 바이너리 도구 배포:

```
tools/cli/
├── com.example.mytools__tool_name/
│   ├── executable → /opt/usr/apps/.../bin/tool
│   └── tool.md    → /opt/usr/apps/.../res/tool.md
```

- 메타데이터 필터: `http://tizen.org/metadata/tizenclaw/cli`
- `.tool.md` 설명서가 시스템 프롬프트에 자동 주입
- `execute_cli` 내장 도구를 통해 `popen()`으로 실행

### `.tool.md` 설명서 형식

```markdown
---
name: my_custom_tool
description: 커스텀 도구 설명
---

## Parameters
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| --command | string | Yes | 실행할 명령 |

## Usage
CLI 도구 사용 방법에 대한 설명
```

---

## LLM 백엔드 플러그인 (RPK)

RPK를 통해 커스텀 LLM 백엔드 동적 배포:

- 커스텀 우선순위 (예: `10`) 지정으로 내장 백엔드 자동 오버라이드
- 제거 시 기본 백엔드로 자연스러운 폴백
- 샘플 프로젝트: [tizenclaw-llm-plugin-sample](https://github.com/hjhun/tizenclaw-llm-plugin-sample)

---

## 멀티 에이전트 생태계

TizenClaw 에이전트는 도구를 통해 협력합니다:

| 도구 | 용도 |
|------|------|
| `create_session` | 커스텀 프롬프트로 전문 에이전트 세션 생성 |
| `send_to_session` | 특정 에이전트에 프롬프트 전달 |
| `run_supervisor` | 목표 기반 멀티 에이전트 분해/위임 |

### A2A 프로토콜

크로스 디바이스 에이전트 통신:

- Agent Card 디스커버리: `/.well-known/agent.json`
- 태스크 생명주기: `submitted` → `working` → `completed`
- JSON-RPC 2.0 + Bearer 토큰 인증
