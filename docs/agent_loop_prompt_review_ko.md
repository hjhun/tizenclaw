# TizenClaw AgentLoop 및 시스템 프롬프트 검토

## 목적

TizenClaw의 현재 `AgentLoop`와 시스템 프롬프트 구성을
`OpenClaw`, `NanoClaw`, `Hermes Agent`와 비교해,
우리 프로젝트에 낮은 위험도로 도입 가능한 패턴을 추려내는 것이
이번 검토의 목적입니다.

핵심 비교 축은 다음 네 가지입니다.

1. 루프 구조와 종료 조건
2. 시스템 프롬프트의 소유권과 계층화 방식
3. 메모리/동적 컨텍스트 주입 위치
4. 서브에이전트, 압축, 안전장치 운영 방식

## TizenClaw 현재 상태

### 1. AgentLoop

현재 TizenClaw는 `AgentLoopState`에 15개 phase를 선언하고
`process_prompt()`에서 이를 순차적으로 전이시키는 구조입니다.

- 상태 선언:
  `src/tizenclaw/src/core/agent_loop_state.rs`
- 실제 루프 진입점:
  `src/tizenclaw/src/core/agent_core.rs`

장점:

- `GoalParsing -> ContextLoading -> Planning -> DecisionMaking ->
  ToolDispatching/ObservationCollect -> Evaluating -> RePlanning ->
  TerminationCheck` 흐름이 명시적입니다.
- `max_tool_rounds`, `idle/stuck detection`, `tool result budget`,
  `context compaction`, `workflow mode`가 이미 들어가 있습니다.
- 시스템 프롬프트 변경 시 `prepare_cache()`를 다시 호출하는 구조라
  프롬프트 캐시 최적화 방향은 잡혀 있습니다.

한계:

- 상태 enum은 풍부하지만 실제 구현은 여전히 하나의 큰 함수 안에
  집중되어 있어, phase별 책임 경계가 약합니다.
- 최종 응답 형식으로 `<think>`와 `<final>`을 강제하는 프롬프트는
  모델별 편차가 큰 편이라, 일부 백엔드에서는 도구 호출 품질이나
  답변 안정성을 떨어뜨릴 수 있습니다.
- 역할 기반 프롬프트(`data/config/agent_roles.json`)와 메인 루프의
  프롬프트 전략이 아직 단일한 정책으로 정렬되어 있지 않습니다.

### 2. 시스템 프롬프트

현재 시스템 프롬프트는 `SystemPromptBuilder`가 구성합니다.

- 파일:
  `src/tizenclaw/src/core/prompt_builder.rs`

구성 특징:

- base prompt
- SOUL 주입
- 추론/도구 사용 규칙
- Safety
- skills / skill references
- runtime context

좋은 점:

- 메모리와 runtime context 일부를 user message 쪽으로 넣어 캐시를
  덜 깨뜨리려는 방향이 보입니다.
- 관련 textual skill prefetch가 이미 있으며, tool catalog도
  시스템 프롬프트에 축약해 싣고 있습니다.

보완점:

- stable prompt와 dynamic prompt의 경계가 아직 뚜렷하지 않습니다.
- sub-agent 또는 role agent용 "작은 프롬프트 모드"가 없습니다.
- 컨텍스트 파일 주입에 대한 공격 표면 검사 체계가 없습니다.

## OpenClaw 분석

### 1. 루프 구조

OpenClaw는 세션당 단일 직렬화 lane을 기준으로 agent run을
처리합니다.

근거:

- `docs/concepts/agent-loop.md`
- `src/auto-reply/reply/commands-system-prompt.ts`
- `src/agents/system-prompt.ts`

핵심 포인트:

- 세션 단위 직렬화(queue lane)가 기본입니다.
- 런타임은 lifecycle/assistant/tool stream을 분리합니다.
- `before_prompt_build`, `before_tool_call`, `before_compaction`,
  `agent_end` 같은 hook 지점이 명확합니다.
- "agent loop"가 단순히 모델 호출 반복이 아니라,
  세션 상태 일관성을 보장하는 운영 파이프라인으로 취급됩니다.

TizenClaw에 주는 시사점:

- 우리도 `process_prompt()` 내부 state 전이만 볼 것이 아니라,
  session serialization, stream emission, hook phase를 별도 계층으로
  끌어내는 편이 유지보수성이 높습니다.

### 2. 시스템 프롬프트

OpenClaw의 가장 강한 부분은 시스템 프롬프트를
"OpenClaw-owned prompt"로 명확히 선언하고,
stable prefix와 dynamic suffix를 구분한다는 점입니다.

근거:

- `docs/concepts/system-prompt.md`
- `src/agents/system-prompt.ts`

핵심 포인트:

- 고정 섹션을 정해둡니다.
  Tooling, Safety, Skills, Workspace, Documentation, Sandbox, Runtime 등
- provider plugin은 전체 프롬프트를 갈아끼우지 않고,
  일부 섹션 override나 stable/dynamic contribution만 추가합니다.
- sub-agent에는 `promptMode=minimal`을 써서 Skills, Heartbeats,
  Self-Update 같은 무거운 섹션을 제거합니다.
- bootstrap 파일은 main agent와 sub-agent에서 다르게 주입합니다.

TizenClaw에 도입 가치가 큰 부분:

1. `full/minimal/none` 프롬프트 모드
2. stable system prompt와 dynamic context의 명확한 분리
3. context weight report처럼 "프롬프트가 왜 무거운지" 보이는 진단
4. provider/backend별 미세 조정은 section override 수준에 한정

주의점:

- OpenClaw의 범용 운영 계층은 풍부하지만, TizenClaw는 embedded
  daemon이므로 그대로 확장하면 prompt와 운영 복잡도가 과해질 수
  있습니다.

## NanoClaw 분석

### 1. 루프 구조

NanoClaw는 host orchestrator는 매우 얇게 두고,
실제 agentic loop를 Claude Agent SDK 내부에 맡깁니다.

근거:

- `README.md`
- `docs/SPEC.md`
- `docs/SDK_DEEP_DIVE.md`
- `container/agent-runner/src/index.ts`

핵심 포인트:

- 바깥쪽은 polling loop + container dispatch + IPC polling입니다.
- 안쪽 agent loop는 Claude Agent SDK의 recursive generator `EZ()`
  입니다.
- 컨테이너 런너는 `query()`를 돌리고,
  결과 후 IPC 메시지를 기다렸다가 같은 세션을 `resume`합니다.
- `resumeSessionAt`에 마지막 assistant UUID를 넘겨,
  subagent/team 실행 후 resume 지점을 안정화합니다.

TizenClaw에 주는 시사점:

- 루프를 모두 자체 구현해야 한다는 강박은 필요 없습니다.
- 우리도 세션 resume 기준점을 더 명시적으로 저장하면,
  follow-up/tool continuation 안정성이 좋아질 수 있습니다.
- loop 본체와 host orchestration을 분리하는 관점은 유용합니다.

### 2. 시스템 프롬프트 / 메모리

NanoClaw는 강한 "자체 시스템 프롬프트"보다
`CLAUDE.md` 계층과 SDK preset을 중심에 둡니다.

핵심 포인트:

- group별 `CLAUDE.md`와 global `CLAUDE.md`를 함께 사용합니다.
- `systemPrompt`는 `claude_code` preset에 global context를 append하는
  방식입니다.
- project/user setting source를 사용해 Claude Code 규약을 그대로
  활용합니다.

TizenClaw에 도입 가능한 부분:

1. 그룹/역할/작업 단위의 경량 메모리 파일 계층화
2. `global + local` 메모리를 합성하되, 실제 prompt 본문은 최소화
3. "세션 재개 기준점"을 명시적으로 저장하는 방식

직접 도입이 어려운 부분:

- Claude Agent SDK 의존 설계
- `CLAUDE.md` 중심의 벤더 종속 흐름
- containerized multi-channel 운영 전제

즉, NanoClaw는 "루프를 외부 SDK에 위임한 미니멀 오케스트레이터"
라는 점이 장점이지만, TizenClaw는 다중 백엔드 Rust 런타임이라
구조를 그대로 가져오기는 어렵습니다.

## Hermes Agent 분석

### 1. 시스템 프롬프트 캐싱 전략

Hermes는 이번 비교군 중
"prompt cache를 깨지 않기 위한 프롬프트 운영"이 가장 정교합니다.

근거:

- `run_agent.py`
- `agent/prompt_builder.py`
- `agent/context_compressor.py`
- `docs/honcho-integration-spec.md`

핵심 포인트:

- 시스템 프롬프트는 세션당 1회 만들고 `_cached_system_prompt`에
  유지합니다.
- 계속되는 세션은 재생성하지 않고 session DB에 저장한 프롬프트를
  재사용합니다.
- memory manager나 plugin hook이 가져온 동적 컨텍스트는
  system prompt가 아니라 "현재 user message"에 붙입니다.
- `ephemeral_system_prompt`도 API call 시점에만 합쳐서,
  캐시된 본문 자체는 건드리지 않습니다.

이건 TizenClaw에 매우 잘 맞습니다.
현재도 일부 dynamic context를 user message에 주입하고 있지만,
Hermes처럼 "stable system prompt는 session 스냅샷으로 고정하고,
변하는 것은 전부 API-call-time overlay로 다룬다"는 규칙이 더
강하게 필요합니다.

### 2. 루프 운영

Hermes는 전형적인 iterative tool loop를 사용하지만,
운영 안전장치가 강합니다.

핵심 포인트:

- `while api_call_count < max_iterations`
- independent iteration budget
- preflight compression
- context length 초과 시 compression retry
- tool batch 병렬화
- subagent도 독립 iteration budget 사용

TizenClaw에 유용한 부분:

1. "본 루프 진입 전" preflight compression
2. tool schema까지 포함한 request size 추정
3. subagent 독립 예산
4. 모델별 tool-use guidance 주입

### 3. 시스템 프롬프트 본문 품질

Hermes는 `TOOL_USE_ENFORCEMENT_GUIDANCE`,
Google model용 운영 지침,
context file injection scan을 제공합니다.

특히 좋은 점:

- 모델 패밀리별로 다른 운영 힌트를 줄 수 있습니다.
- AGENTS/SOUL 같은 로컬 문서를 시스템 프롬프트에 넣기 전에
  간단한 prompt injection 탐지를 합니다.

TizenClaw에 필요한 이유:

- 현재 TizenClaw의 `<think>/<final>` 강제 규칙은 강하지만,
  모델별 운영 편차를 흡수하는 guidance 레이어는 약합니다.
- skills / soul / user documents가 늘어날수록
  로컬 컨텍스트 파일이 prompt injection 진입점이 될 수 있습니다.

## 종합 비교

### OpenClaw가 가장 강한 영역

- 시스템 프롬프트 계층 설계
- sub-agent용 minimal prompt
- hook 지점과 운영 파이프라인 분리
- context diagnostics

### NanoClaw가 가장 강한 영역

- 오케스트레이터를 얇게 두는 단순성
- group별 격리 메모리/세션
- resume anchor 관리

### Hermes가 가장 강한 영역

- 캐시 안정적인 prompt 운영
- dynamic context를 user message로 미루는 전략
- compression/retry/budget 운영
- 모델별 tool-use enforcement
- context file injection 방어

## TizenClaw 도입 권고

### 우선순위 P0

1. **프롬프트 계층 분리 강화**
   - stable core system prompt
   - subagent/role agent용 minimal prompt
   - dynamic runtime/memory/plugin context는 user message overlay

2. **`<think>/<final>` 강제 정책 완화 또는 backend-aware화**
   - 모든 백엔드에 동일 강제를 걸기보다,
     reasoning 지원 모델과 비지원 모델을 나눠 정책화하는 편이
     안전합니다.

3. **세션 단위 system prompt snapshot 유지**
   - Hermes처럼 세션 지속 중에는 동일 프롬프트를 재사용하고,
     재구성은 compression 또는 설정 변경 시점으로 제한하는 것이
     좋습니다.

### 우선순위 P1

4. **sub-agent / role-agent prompt mode 도입**
   - OpenClaw처럼 `full/minimal/none` 모드를 두고,
     `agent_roles.json` 계열 프롬프트도 이 정책 아래 정리합니다.

5. **context file scan**
   - `SOUL.md`, `AGENTS.md`, 향후 workspace memory 파일을
     프롬프트에 넣기 전 간단한 injection 패턴 스캔을 추가합니다.

6. **preflight compression**
   - 본 루프 진입 전에 system prompt + tool schema + messages 전체를
     계산해 선제 압축합니다.

### 우선순위 P2

7. **resume anchor / continuation marker**
   - NanoClaw처럼 마지막 assistant turn 기준 재개 지점을 명시적으로
     관리하면 follow-up 안정성이 좋아집니다.

8. **독립 subagent iteration budget**
   - 향후 supervisor/subagent 구조가 커질 경우 Hermes 방식이
     유용합니다.

9. **context diagnostics command/report**
   - OpenClaw처럼 어떤 파일/skills/tools가 context를 얼마나 먹는지
     보여주는 보고 기능이 필요합니다.

## 비도입 또는 신중 검토 항목

1. NanoClaw의 Claude Agent SDK 직접 의존
   - TizenClaw는 multi-backend Rust 런타임이라 맞지 않습니다.

2. OpenClaw 수준의 광범위한 운영 섹션을 그대로 복제
   - embedded daemon 문맥에서는 prompt가 너무 비대해질 수 있습니다.

3. Hermes의 전체 플러그인/메모리 체계를 한 번에 도입
   - 먼저 prompt layering과 compression 정책부터 가져오는 편이
     안전합니다.

## 결론

TizenClaw는 이미 `AgentLoopState`, tool result budget,
context compaction, prompt cache preparation 같은 기반을 갖고 있어
출발점은 나쁘지 않습니다.

다만 현재 구조는
"루프 상태는 풍부하지만 프롬프트 운영 원칙은 아직 단일 정책으로
정제되지 않은 상태"에 가깝습니다.

가장 효과가 큰 다음 단계는 다음 세 가지입니다.

1. OpenClaw식 `prompt mode` 도입
2. Hermes식 stable prompt / dynamic overlay 분리
3. `<think>/<final>` 규칙을 backend-aware 정책으로 재정의

이 세 가지가 먼저 들어가면, 이후의 role-agent 정리,
subagent 최소 프롬프트, context diagnostics, resume anchor 도입도
훨씬 수월해질 것입니다.
