# TizenClaw 멀티 에이전트 및 퍼셉션 로드맵

> **날짜**: 2026-03-22
> **참고**: [프로젝트 분석](ANALYSIS.md) | [설계 문서](DESIGN.md)

---

## 1. 개요

TizenClaw가 제한된 임베디드 환경에서 복잡하고 장시간 실행되는 운영 워크플로우를 처리하도록 성숙해짐에 따라, 단일 세션 기반 에이전트 접근법에서 고도로 분산되고 안정적인 **11개 MVP 에이전트 세트**와 고급 **퍼셉션 레이어**로의 전환이 진행되고 있습니다.

---

## 2. Phase A: MVP 에이전트 세트 구성

### 11개 에이전트 MVP 세트

임베디드 디바이스에서의 운영 안정성을 달성하기 위해, 기존 Orchestrator와 Skill Manager를 7개 카테고리의 11개 전문 역할로 분할합니다.

| 카테고리 | 에이전트 | 주요 책임 |
|----------|---------|---------|
| **이해** | `Input Understanding Agent` | 모든 채널의 사용자 입력을 통합 인텐트 구조로 표준화 |
| **인식** | `Environment Perception Agent` | 이벤트 버스를 구독하여 공통 상태 스키마 유지 |
| **기억** | `Session / Context Agent` | 단기(현재 작업), 장기(사용자 선호), 에피소드 메모리 관리 |
| **판단** | `Planning Agent` (오케스트레이터) | Capability Registry를 기반으로 목표를 논리적 단계로 분해 |
| **실행** | `Action Execution Agent` | OCI 컨테이너 스킬 및 Action Framework 명령 호출 |
| **보호** | `Policy / Safety Agent` | 실행 전 계획을 가로채어 제약(야간 제한 등) 적용 |
| **유틸리티** | `Knowledge Retrieval Agent` | SQLite RAG 저장소를 통한 시맨틱 검색 인터페이스 |
| **모니터링** | `Health Monitoring Agent` | 메모리 압박(PSS), 데몬 업타임, 컨테이너 상태 모니터링 |
| | `Recovery Agent` | 구조화된 실패(예: DNS 타임아웃) 분석 및 폴백/오류 수정 |
| | `Logging / Trace Agent` | 디버깅 및 감사 로그 중앙화 |

*(기존 `Skill Manager` 에이전트는 RPK 기반 도구 배포가 성숙해짐에 따라 실행/복구 레이어로 흡수 예정)*

---

## 3. Phase B: 퍼셉션 아키텍처 구현

강건한 멀티 에이전트 시스템은 고품질 퍼셉션에 의존합니다. TizenClaw의 퍼셉션 레이어는 다음 핵심 원칙으로 설계됩니다:

### 3.1 공통 상태 스키마
원시 `/proc` 데이터나 분산된 로그를 연속적인 JSON 스키마로 정규화:
- `DeviceState`: 활성 기능 (디스플레이, BT, WiFi), 모델명
- `RuntimeState`: 네트워크 상태, 메모리 압박, 전원 모드
- `UserState`: 로캘, 선호설정, 역할
- `TaskState`: 현재 목표, 활성 단계, 누락된 인텐트 슬롯

### 3.2 Capability Registry 및 Function Contract
모든 동적 RPK 플러그인, CLI 도구, 내장 스킬은 입력/출력 스키마, 부작용, 재시도 정책, 필요 권한 등을 명시하는 구조화된 Capability Registry에 등록해야 합니다.

### 3.3 이벤트 버스 (이벤트 기반 업데이트)
지속적 폴링 대신 세분화된 이벤트(예: `sensor.changed`, `network.disconnected`, `action.failed`)에 반응하여 CPU 소모 없이 상태를 최신화합니다.

### 3.4 격리된 메모리 구조
- *단기*: 현재 대화, 최근 명령, 즉각적 실패 원인
- *장기*: 사용자 선호, 일반적 사용 패턴
- *에피소드*: 특정 조건에서 스킬 실행 성공/실패 이력

### 3.5 임베디드 설계 원칙
- **선택적 컨텍스트 주입**: LLM에 필요한 상태만 제공. `[network: disconnected, reason: dns_timeout]`이 1,000줄의 `dlog`보다 효과적
- **인식과 실행의 분리**: Perception Agent가 상태를 읽고, Execution Agent가 실행
- **확신도 점수**: 인텐트/객체 감지에 확신도(예: `confidence: 0.82`)를 부여하여 불확실 시 확인 질문

---

## 4. Phase C: RPK 도구 배포를 통한 확장성

구조화된 capability와 함수 계약으로의 전환과 함께, 최종 단계에서 **RPK 도구 배포**를 도입합니다.

Tizen Resource Package (RPK)가 번들링하는 항목:
- 샌드박스된 Python 스킬
- 호스트/컨테이너 CLI 기반 도구

이 패키지들은 데몬 재컴파일 없이 `Capability Registry`를 동적으로 채웁니다.
