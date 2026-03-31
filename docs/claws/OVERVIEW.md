# Claw 생태계 분석 요약 보드 (Overview Dashboard)

이 문서는 OpenClaw, NanoClaw, 그리고 Hermes Agent 세 가지 주요 에이전트 시스템을 한눈에 비교하여 TizenClaw 프로젝트의 방향성 설정에 도움을 주기 위해 작성되었습니다.

## 1. 종합 비교표 (Comparison Matrix)

| 구분 (Category) | **OpenClaw** | **NanoClaw** | **Hermes Agent** |
| :--- | :--- | :--- | :--- |
| **핵심 철학** | 확장 가능한 에이전트 플랫폼 | 이해하기 쉬운 단순한 에이전트 | 인프라 상주형 고성능 에이전트 |
| **규모/복잡도** | 중상 (모듈형 모노레포) | 최하 (단일 프로세스) | 상 (엔터프라이즈 급) |
| **주요 인터페이스** | Web 대시보드 / API | Claude Code CLI | 메시징 앱 (Telegram 등) |
| **기술 스택** | TypeScript, React, Docker | Python, JavaScript, uv | Python, Node.js, Nix, MCP |
| **최대 강점** | 스케일링과 UI 가시성 | 낮은 진입장벽과 분석 용이성 | 40+ 기본 도구 및 영속적 메모리 |
| **TizenClaw 참조** | UI 레이아웃 및 모듈 구분 | 경량화 및 제로설정 배포 기법 | C-API 스킬 뱅크 및 보안 구조 |

## 2. 에이전트별 추천 활용 시나리오 (Use-Cases)

### [OpenClaw] - 시각화 및 대형 시스템 연동
- 다수의 에이전트를 통합 관리해야 하는 상황
- 웹 기반의 관제 시스템이 필수적인 엔터프라이즈 환경

### [NanoClaw] - 빠른 실험 및 학습
- AI 에이전트의 내부 동작 원리를 깊게 파악하고 싶은 개발자
- 리소스가 극도로 제한된 환경에서의 프로토타이핑

### [Hermes Agent] - 실질적인 업무 자동화 및 비서 서비스
- 이미 사용 중인 메신저를 통해 기기를 제어하고 싶은 경우
- 복잡한 도구 활용이 필요한 높은 수준의 자율 작업 수행

---

## 3. 결론 및 TizenClaw의 방향성
TizenClaw는 **NanoClaw의 경량화 정신**을 계승하면서도, **Hermes Agent의 강력한 도구 관리 및 보안 체계**를 Tizen Native 환경에 이식하는 것을 목표로 삼아야 합니다. 특히 **OpenClaw의 모듈형 구조**를 차용하여 Tizen C-API 기반의 스킬들이 원활하게 확장될 수 있는 생태계를 구축하는 것이 핵심입니다.
