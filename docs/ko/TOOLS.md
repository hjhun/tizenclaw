# TizenClaw 도구 레퍼런스

TizenClaw는 **35개 컨테이너 스킬** (Python, OCI 샌드박스), **10개 이상의 내장 도구** (네이티브 C++), 그리고 **CLI 도구 플러그인** (TPK 기반 네이티브 실행 파일)을 제공합니다.

> 컨테이너 스킬은 `ctypes` FFI를 통해 Tizen C-API를 직접 호출합니다. 비동기 스킬은 **tizen-core** 이벤트 루프를 사용합니다.

---

## 컨테이너 스킬 (Python)

### 앱 관리

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `list_apps` | — | `app_manager` | 설치된 앱 목록 조회 |
| `send_app_control` | `app_id`, `operation`, `uri`, `mime`, `extra_data` | `app_control` | 명시적 app_id 또는 암시적 인텐트(operation/URI/MIME)로 앱 실행 |
| `terminate_app` | `app_id` | `app_manager` | 실행 중인 앱 종료 |
| `get_package_info` | `package_id` | `package_manager` | 패키지 상세 정보 (버전, 타입, 크기) |

### 디바이스 정보 & 센서

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_device_info` | — | `system_info` | 모델, OS 버전, 플랫폼 정보 |
| `get_system_info` | — | `system_info` | 하드웨어 상세 (CPU, 화면, 기능) |
| `get_runtime_info` | — | `runtime_info` | CPU/메모리 사용량 |
| `get_storage_info` | — | `storage` | 내부/외부 저장소 공간 |
| `get_system_settings` | — | `system_settings` | 로케일, 시간대, 글꼴, 배경화면 |
| `get_sensor_data` | `sensor_type` | `sensor` | 가속도계, 자이로, 조도, 근접 센서 등 |
| `get_thermal_info` | — | `device` (thermal) | 디바이스 온도 (AP, CP, 배터리) |

### 네트워크 & 연결

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_wifi_info` | — | `wifi-manager` | 현재 WiFi 연결 상세 |
| `get_bluetooth_info` | — | `bluetooth` | 블루투스 어댑터 상태 |
| `get_network_info` | — | `connection` | 네트워크 타입, IP 주소, 상태 |
| `get_data_usage` | — | `connection` (통계) | WiFi/셀룰러 데이터 사용량 |
| `scan_wifi_networks` | — | `wifi-manager` + **tizen-core** ⚡ | 주변 WiFi 액세스 포인트 스캔 (비동기) |
| `scan_bluetooth_devices` | `action` | `bluetooth` + **tizen-core** ⚡ | 주변 BT 장치 탐색 또는 페어링 목록 (비동기) |

### 디스플레이 & 하드웨어 제어

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_display_info` | — | `device` (display) | 밝기, 상태, 최대 밝기 |
| `control_display` | `brightness` | `device` (display) | 디스플레이 밝기 설정 |
| `control_haptic` | `duration_ms` | `device` (haptic) | 디바이스 진동 |
| `control_led` | `action`, `brightness` | `device` (flash) | 카메라 플래시 LED on/off |
| `control_volume` | `action`, `sound_type`, `volume` | `sound_manager` | 볼륨 레벨 조회/설정 |
| `control_power` | `action`, `resource` | `device` (power) | CPU/디스플레이 잠금 요청/해제 |

### 미디어 & 콘텐츠

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `get_battery_info` | — | `device` (battery) | 배터리 잔량 및 충전 상태 |
| `get_sound_devices` | — | `sound_manager` (device) | 오디오 디바이스 목록 (스피커, 마이크) |
| `get_media_content` | `media_type`, `max_count` | `media-content` | 디바이스 미디어 파일 검색 |
| `get_metadata` | `file_path` | `metadata-extractor` | 미디어 파일 메타데이터 추출 (제목, 아티스트, 앨범, 길이 등) |
| `get_mime_type` | `file_extension`, `file_path`, `mime_type` | `mime-type` | MIME 타입 ↔ 확장자 조회 |

### 시스템 액션

| 스킬 | 파라미터 | C-API | 설명 |
|------|---------|-------|------|
| `play_tone` | `tone`, `duration_ms` | `tone_player` | DTMF/비프 톤 재생 |
| `play_feedback` | `pattern` | `feedback` | 사운드/진동 피드백 패턴 재생 |
| `send_notification` | `title`, `body` | `notification` | 디바이스 알림 게시 |
| `schedule_alarm` | `app_id`, `datetime` | `alarm` | 특정 시간에 알람 예약 |
| `download_file` | `url`, `destination`, `file_name` | `url-download` + **tizen-core** ⚡ | URL에서 파일 다운로드 (비동기) |
| `web_search` | `query` | — (Wikipedia) | Wikipedia API 웹 검색 |

> ⚡ = **tizen-core** 이벤트 루프를 사용하는 비동기 스킬 (`tizen_core_task_create` → `add_idle_job` → `task_run` → 콜백 → `task_quit`)

---

## 내장 도구 (AgentCore, 네이티브 C++)

| 도구 | 설명 |
|------|------|
| `execute_code` | 샌드박스에서 Python 코드 실행 |
| `file_manager` | 디바이스 파일 읽기/쓰기/조회 |
| `manage_custom_skill` | 런타임 커스텀 스킬 생성/수정/삭제/조회 |
| `create_task` | 예약 작업 생성 |
| `list_tasks` | 활성 예약 작업 조회 |
| `cancel_task` | 예약 작업 취소 |
| `create_session` | 새 채팅 세션 생성 |
| `list_sessions` | 활성 세션 조회 |
| `send_to_session` | 다른 세션에 메시지 전송 |
| `ingest_document` | RAG 스토어에 문서 인덱싱 |
| `search_knowledge` | RAG 스토어 시맨틱 검색 |
| `execute_action` | Tizen Action Framework 액션 실행 |
| `action_<name>` | Per-action 도구 (Action Framework에서 자동 발견) |
| `execute_cli` | TPK 패키지로 설치된 CLI 도구 플러그인 실행 |

---

## RPK 도구 배포 및 확장성 (RPK Tool Distribution)

TizenClaw의 기능 생태계는 내장 도구를 넘어 **Tizen Resource Packages (RPK)** 를 통해 확장됩니다. 이는 기업 환경에서의 구조화된 배포 메커니즘을 제공함으로써 기존의 `manage_custom_skill` 방식을 대체/승계합니다.

RPK 도구 패키지는 다음을 포함할 수 있습니다:
1. **샌드박스 처리된 Python 스킬 (Sandboxed Python Skills)**: OCI 컨테이너 내부에서 안전하게 실행되는 신규 도구.
2. **호스트/컨테이너 CLI 도구 (Host/Container CLI Tools)**: `execute_action`이나 `execute_code`를 통하여 호출되는 바이너리 유틸리티 및 스크립트.

### Capability Registry
모든 동적 RPK 플러그인, CLI 툴 및 내장 스킬은 TizenClaw의 단일화된 **Capability Registry**에 의무적으로 등록되어야 합니다. 이것은 다음을 보장합니다:
- 명확한 **함수 계약 (Function Contracts)** (입력/출력 JSON 스키마 보장).
- 정의된 부작용(Side effects) 및 재시도 정책 수립.
- 필수적인 샌드박스 및 Tizen 시스템 보안(SMACK) 권한 규정.

시스템 패키지 관리자(예: `pkgcmd`)를 통해 RPK가 설치되면 TizenClaw는 즉시 이를 감지하고 등록 기능을 Planning Agent가 이용할 수 있도록 노출합니다. 별도의 데몬 재컴파일은 필요하지 않습니다.

---

## CLI 도구 플러그인 (TPK 기반)

Python 스킬 외에도, TizenClaw는 **TPK (Tizen Package)** 로 패키징된 **네이티브 CLI 도구 플러그인**을 지원합니다. CLI 도구는 Tizen C-API에 직접 접근하기 위해 호스트에서 직접 실행되며, 각 도구는 LLM이 커맨드, 인자, 출력 형식을 이해할 수 있도록 `.tool.md` 설명서를 포함합니다.

### 아키텍처

| 컴포넌트 | 역할 |
|----------|------|
| `CliPluginManager` | `http://tizen.org/metadata/tizenclaw/cli` 메타데이터를 가진 TPK를 발견하고 `tools/cli/`에 symlink 생성 |
| `tizenclaw-metadata-cli-plugin.so` | 설치 시 플랫폼 수준 인증서 서명을 강제하는 파서 플러그인 |
| `execute_cli` (내장 도구) | `popen()`을 통해 CLI 도구를 실행하고 JSON 출력을 LLM에 반환 |
| `.tool.md` 설명서 | 시스템 프롬프트에 주입되는 리치 Markdown 파일로 LLM 도구 발견 지원 |

### 매니페스트 선언

CLI 도구는 `tizen-manifest.xml`에서 `<service-application>`을 사용합니다:

```xml
<service-application appid="org.tizen.sample.get_package_info"
                     exec="get_package_info" type="capp">
    <metadata key="http://tizen.org/metadata/tizenclaw/cli"
              value="get_package_info"/>
</service-application>
```

> **보안**: 플랫폼 서명된 TPK만 CLI 도구를 등록할 수 있습니다.

---

## 멀티 에이전트 생태계 (Multi-Agent Ecosystem)

TizenClaw는 분산된 **11개의 MVP 에이전트 환경**을 통해 신뢰할 수 있게 요청을 수행하고 디바이스 상태를 관리합니다:

| 카테고리 | 에이전트 | 주요 책임 |
|----------|----------|-----------|
| **이해** | `Input Understanding Agent` | 모든 채널의 사용자 입력을 단일한 인텐트(Intent) 구조로 표준화. |
| **인식** | `Environment Perception Agent` | 이벤트 버스를 구독하여 공통 상태 스키마(Common State Schema) 유지. |
| **기억** | `Session / Context Agent` | 단기 기억(현재 작업), 장기 기억(사용자 선호), 에피소드 기억 관리. |
| **판단** | `Planning Agent` | 퍼셉션과 Capability Registry 기반 목표를 논리적 단계로 분해. |
| **실행** | `Action Execution Agent` | 실제 OCI 컨테이너 스킬 및 Action Framework 명령 호출 수행. |
| **보호** | `Policy / Safety Agent` | 실행 전 계획을 가로채어 정책(샌드박스 제한 등) 시행. |
| **유틸리티** | `Knowledge Retrieval Agent` | 시맨틱 검색용 SQLite RAG 저장소 인터페이스. |
| **모니터링** | `Health Monitoring Agent` | 메모리 압박(PSS) 및 컨테이너 건전성 등 모니터링 관리. |
| | `Recovery Agent` | 구조적 실패 분석 및 LLM 기반의 폴백 또는 오류 교정 시도. |
| | `Logging / Trace Agent` | 디버깅 및 감사 기록을 위한 컨텍스트 중앙화 수행. |

에이전트들은 공유된 `이벤트 버스(Event Bus)`를 활용해 상호작용하며 내부 통신을 이룹니다. 이 중 *Planning Agent*가 중심이 되어 실시간 Perception 상태 기반으로 사용자 의도를 동작 단계로 번역합니다.

---

## 비동기 패턴 (tizen-core)

⚡ 표시된 스킬은 콜백 기반 Tizen API를 위한 비동기 패턴을 사용합니다:

```
tizen_core_init()
  → tizen_core_task_create("main", false)
    → tizen_core_add_idle_job(API_호출_시작)
    → tizen_core_add_timer(타임아웃_ms, 안전_타임아웃)
    → tizen_core_task_run()          ← quit까지 블록
      → API 콜백 실행
        → 결과 수집
        → tizen_core_task_quit()
  → 결과 반환
```

이를 통해 Python FFI에서 스레딩 없이 모든 콜백 기반 Tizen C-API 사용 가능.
