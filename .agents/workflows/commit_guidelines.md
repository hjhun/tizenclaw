---
description: TizenClaw Git Commit Guidelines
---

# TizenClaw Commit Message Workflow

TizenClaw 프로젝트(Phase 2 ~ Phase 5) 개발 중 자동화된 단계별 커밋 시에는 항상 다음 규칙을 엄격히 준수해야 합니다.

## 1. 커밋 메시지 기본 구조
Conventional Commits 스타일과 유사하게 목적이 명확히 드러나는 제목을 작성하고, 본문을 한 줄 이상의 단락으로 분리합니다. 
원치 않는 추가 텍스트(예: "Verification", "Testing Results:" 등 봇이 만들어내는 불필요한 장황한 문구)는 절대 포함하지 않습니다.

```text
[Phase X] 제목 (50자 이내, 영문/한글 혼용 가능하지만 간결하게)

(빈 줄)
구현된 기능, 수정된 버그, 또는 구조적 변경 사항에 대한 구체적인 내용을 작성합니다.
왜(Why)와 무엇을(What) 했는지 위주로 짧게 명시합니다. (72자마다 줄바꿈)
```

## 2. 작성 예시 (Good)
```text
[Phase 2] Add LXC container engine integration to AgentCore

AgentCore 클래스 내부에 LXC 컨테이너 설정 및 시작/종료 인터페이스를
구현했습니다. C++ lxc 라이브러리를 바인딩하여 샌드박스의 기반을 마련했습니다.
```

## 3. 금지 사항 (Bad)
다음과 같은 기계적이고 불필요한 Verification 텍스트 블록은 **절대 넣지 마세요.**
```text
[Phase 2] Add LXC ...
(빈 줄)
내용...
(빈 줄)
Verification:
- gbs build passed
- ctest passed
- 100% lines covered
```
*-> 이러한 로그 기반 확인 텍스트는 커밋 메시지에 남기지 않고 터미널 출력이나 PR 리뷰용으로만 시각적으로 확인합니다.*

## 4. 커밋 타이밍 (Workflow)
1. 문서(`implementation_phases.md` 등)에 명시된 단위 기능 1개가 구현됨
2. `gbs build` (내부 `%check`의 gtest 포함)가 완벽하게 에러 없이 통과됨 (에러 발생 시 코드 수정)
3. 해당 워크플로 룰에 맞춰 `git add .` 후 `git commit -m "$(상기 포맷의 메시지)"` 수행

// turbo-all
