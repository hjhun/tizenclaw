---
description: TizenClaw Git Commit Guidelines
---

# TizenClaw Commit Message Workflow

TizenClaw 프로젝트(Phase 2 ~ Phase 5) 개발 중 자동화된 단계별 커밋 시에는 항상 다음 규칙을 엄격히 준수해야 합니다.

## 1. 커밋 메시지 기본 구조
Conventional Commits 스타일과 유사하게 목적이 명확히 드러나는 제목을 작성하고, 본문을 한 줄 이상의 단락으로 분리합니다. 
원치 않는 추가 텍스트(예: "Verification", "Testing Results:" 등 봇이 만들어내는 불필요한 장황한 문구)는 절대 포함하지 않습니다.
**가장 중요한 점: 커밋 메시지는 항상 영어(English)로 작성되어야 하며, 변경 사항에 대한 구체적이고 상세한 설명이 본문에 포함되어야합니다.**

```text
Title (Under 50 chars, clear and concise English)

(빈 줄)
Provide a detailed explanation of the implemented features, bug fixes, or structural changes.
Describe 'Why' and 'What' was done extensively but clearly. (Wrap text at 72 characters)
```

## 2. 작성 예시 (Good)
```text
Switch from LXC to lightweight runc for ContainerEngine

Refactored the ContainerEngine implementation to use the lightweight
`runc` CLI via `std::system` instead of relying on `liblxc` APIs.
This change was necessary because the Tizen 10 GBS build environment
does not provide the `pkgconfig(lxc)` dependency. The new implementation
successfully parses the container name and rootfs path to construct
and spawn robust runc commands.
```

## 3. 금지 사항 (Bad)
다음과 같은 기계적이고 불필요한 Verification 텍스트 블록은 **절대 넣지 마세요.**
```text
Add LXC ...
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