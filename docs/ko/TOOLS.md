# TizenClaw 도구 레퍼런스 — Python 포팅

> **최종 업데이트**: 2026-03-23
> **브랜치**: `develPython`

TizenClaw Python 포팅은 **13개 네이티브 CLI 도구 스위트** (C++ 버전과 공유하는 독립 실행 파일)와 **17개 내장 도구 MD 스키마**를 제공합니다. 모든 도구는 데몬 시작 시 `ToolIndexer`가 `.tool.md`, `.skill.md`, `.mcp.json` 파일을 스캔하여 발견합니다.

> **도구 발견**: `ToolIndexer` 클래스는 정규식으로 Markdown 스키마 파일의 YAML frontmatter를 파싱하여 `name`과 `description` 필드를 추출합니다. 각 도구는 유연한 LLM 호출을 위해 catch-all `arguments` 파라미터를 갖습니다.

> CLI 도구 스위트는 `ctypes` FFI를 사용하여 Tizen C-API를 직접 호출합니다. 비동기 스킬(⚡)은 **tizen-core** 이벤트 루프를 사용합니다.

---

## 도구 아키텍처 (Python 포팅)

```
AgentCore
    │
    ▼
ToolIndexer                          ToolDispatcher
(파일시스템 스키마 스캔)              (도구 호출 라우팅)
    │                                     │
    ├── tools/cli/*/*.tool.md        ┌────┤
    ├── tools/embedded/*.md          │    │
    └── *.mcp.json                   │    │
                                     ▼    ▼
                            ContainerEngine
                            (abstract UDS IPC)
                                     │
                                     ▼
                            Tool Executor
                            (asyncio 서브프로세스)
                                     │
                                     ▼
                            CLI 바이너리 / Python 스크립트
                            (Tizen C-API via ctypes)
```

### C++ 대비 도구 시스템 비교

| 기능 | C++ (main/devel) | Python (develPython) |
|------|:---:|:---:|
| **ToolIndexer** | C++ YAML 파서 | Python 정규식 YAML 파서 |
| **ToolDispatcher** | `std::unordered_map` + `std::shared_mutex` | `Dict` + `asyncio.Lock` |
| **CapabilityRegistry** | ✅ FunctionContract 시스템 | 🔴 미포팅 |
| **CLI 실행** | `popen()` via C++ | `asyncio.create_subprocess_exec` |
| **컨테이너 런타임** | crun 1.26 (OCI) | `unshare` 폴백 |
| **플러그인 발견** | pkgmgrinfo (RPK/TPK) | 🔴 미포팅 |
| **스킬 핫리로드** | inotify 감시자 | 🔴 미포팅 |
| **`.tool.md` 형식** | 동일 | 동일 |

---

## 네이티브 CLI 도구 스위트 (13개 디렉토리)

C++ 버전과 Python 데몬 모두 동일한 독립 실행 CLI 도구를 공유합니다. `tools/cli/`에 위치하며 `ctypes` FFI로 Tizen C-API와 인터페이스합니다.

### 앱 관리

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `list_apps` | — | `app_manager` | 설치된 모든 앱 목록 |
| `send_app_control` | `app_id`, `operation`, `uri`, `mime`, `extra_data` | `app_control` | 앱 실행 (explicit/implicit) |
| `terminate_app` | `app_id` | `app_manager` | 실행 중인 앱 종료 |
| `get_package_info` | `package_id` | `package_manager` | 패키지 상세 정보 |

### 디바이스 정보 및 센서

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_device_info` | — | `system_info` | 모델, OS 버전, 플랫폼 정보 |
| `get_system_info` | — | `system_info` | 하드웨어 세부사항 |
| `get_runtime_info` | — | `runtime_info` | CPU 및 메모리 사용 통계 |
| `get_storage_info` | — | `storage` | 내장/외장 저장 공간 |
| `get_system_settings` | — | `system_settings` | 로캘, 시간대, 글꼴, 벽지 |
| `get_sensor_data` | `sensor_type` | `sensor` | 가속도계, 자이로, 밝기, 근접 등 |
| `get_thermal_info` | — | `device` (thermal) | 디바이스 온도 (AP, CP, 배터리) |

### 네트워크 및 연결

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_wifi_info` | — | `wifi-manager` | 현재 WiFi 연결 정보 |
| `get_bluetooth_info` | — | `bluetooth` | 블루투스 어댑터 상태 |
| `get_network_info` | — | `connection` | 네트워크 유형, IP 주소 |
| `get_data_usage` | — | `connection` (statistics) | WiFi/셀룰러 데이터 사용량 |
| `scan_wifi_networks` | — | `wifi-manager` + **tizen-core** ⚡ | 근처 WiFi AP 스캔 (비동기) |
| `scan_bluetooth_devices` | `action` | `bluetooth` + **tizen-core** ⚡ | BT 디바이스 발견/페어링 목록 (비동기) |

### 디스플레이 및 하드웨어 제어

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_display_info` | — | `device` (display) | 밝기, 상태, 최대 밝기 |
| `control_display` | `brightness` | `device` (display) | 디스플레이 밝기 설정 |
| `control_haptic` | `duration_ms` | `device` (haptic) | 디바이스 진동 |
| `control_led` | `action`, `brightness` | `device` (flash) | 카메라 플래시 LED 제어 |
| `control_volume` | `action`, `sound_type`, `volume` | `sound_manager` | 음량 조절 |
| `control_power` | `action`, `resource` | `device` (power) | CPU/디스플레이 잠금 요청/해제 |

### 미디어 및 콘텐츠

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_battery_info` | — | `device` (battery) | 배터리 잔량 및 충전 상태 |
| `get_sound_devices` | — | `sound_manager` (device) | 오디오 디바이스 목록 |
| `get_media_content` | `media_type`, `max_count` | `media-content` | 미디어 파일 검색 |
| `get_metadata` | `file_path` | `metadata-extractor` | 미디어 파일 메타데이터 추출 |
| `get_mime_type` | `file_extension`, `file_path`, `mime_type` | `mime-type` | MIME 타입 ↔ 확장자 조회 |

### 시스템 액션

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `play_tone` | `tone`, `duration_ms` | `tone_player` | DTMF 또는 비프음 재생 |
| `play_feedback` | `pattern` | `feedback` | 사운드/진동 패턴 재생 |
| `send_notification` | `title`, `body` | `notification` | 디바이스에 알림 게시 |
| `schedule_alarm` | `app_id`, `datetime` | `alarm` | 특정 시간에 알람 예약 |
| `download_file` | `url`, `destination`, `file_name` | `url-download` + **tizen-core** ⚡ | URL 파일 다운로드 (비동기) |
| `web_search` | `query` | — (Wikipedia) | Wikipedia API 웹 검색 |

> ⚡ = **tizen-core** 이벤트 루프를 사용하는 비동기 스킬

---

## 내장 도구 스키마 (17개 파일)

`tools/embedded/`에 위치한 Markdown 파일로, `ToolIndexer`가 LLM 도구 발견을 위해 로드합니다. Python 포팅에서 이 파일들은 읽기 전용 스키마 정의이며, 실제 실행 로직은 `ToolDispatcher`가 처리합니다.

| 도구 | 파일 | 카테고리 |
|------|------|---------|
| `execute_code` | `execute_code.md` | 코드 실행 |
| `create_task` | `create_task.md` | 태스크 스케줄러 |
| `list_tasks` | `list_tasks.md` | 태스크 스케줄러 |
| `cancel_task` | `cancel_task.md` | 태스크 스케줄러 |
| `create_session` | `create_session.md` | 멀티 에이전트 |
| `ingest_document` | `ingest_document.md` | RAG |
| `search_knowledge` | `search_knowledge.md` | RAG |
| `create_workflow` | `create_workflow.md` | 워크플로우 엔진 |
| `list_workflows` | `list_workflows.md` | 워크플로우 엔진 |
| `run_workflow` | `run_workflow.md` | 워크플로우 엔진 |
| `delete_workflow` | `delete_workflow.md` | 워크플로우 엔진 |
| `create_pipeline` | `create_pipeline.md` | 파이프라인 엔진 |
| `list_pipelines` | `list_pipelines.md` | 파이프라인 엔진 |
| `run_pipeline` | `run_pipeline.md` | 파이프라인 엔진 |
| `delete_pipeline` | `delete_pipeline.md` | 파이프라인 엔진 |
| `run_supervisor` | `run_supervisor.md` | 멀티 에이전트 |
| `generate_web_app` | `generate_web_app.md` | 웹 앱 |

> **C++ 대비 부족**: C++ 버전은 `execute_action`, `action_<name>`, `execute_cli`, `manage_custom_skill`, `list_sessions`, `send_to_session`, `remember`, `recall`, `forget` 등 추가 내장 도구를 네이티브로 구현합니다.

---

## 비동기 패턴 (tizen-core)

⚡ 표시된 스킬은 Tizen 콜백 기반 API용 비동기 패턴을 사용합니다:

```
tizen_core_init()
  → tizen_core_task_create("main", false)
    → tizen_core_add_idle_job(start_api_call)
    → tizen_core_add_timer(timeout_ms, safety_timeout)
    → tizen_core_task_run()          ← quit까지 블로킹
      → API 콜백 발동
        → 결과 수집
        → tizen_core_task_quit()
  → 결과 반환
```

이 패턴으로 Python FFI가 스레딩 없이 모든 콜백 기반 Tizen C-API를 사용할 수 있습니다.
