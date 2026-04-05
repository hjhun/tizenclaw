# TizenClaw CLI PinchBench 설정 가이드

이 문서는 `tizenclaw-cli`로 `llm_config.json`을 관리하면서
PinchBench 실행에 필요한 Anthropic/Gemini 설정을 맞추는 방법을
설명합니다. OpenClaw 또는 ZeroClaw에서 `config set/get`을 쓰던
흐름과 비슷하게 사용할 수 있도록 정리했습니다.

## 전제

- `tizenclaw` daemon이 실행 중이어야 합니다.
- `tizenclaw-cli`가 PATH에 있어야 합니다.
- 런타임 설정은 daemon의 `llm_config.json`에 저장됩니다.
- 사용자 스킬의 canonical 경로는 `~/.tizenclaw/workspace/skills`
  입니다. host install은 기존 `~/.tizenclaw/tools/skills` 를 이
  경로로 연결해 하위 호환을 유지합니다.
- `tizenclaw-cli config set`은 기본적으로 문자열을 저장합니다.
- 숫자, 배열, 객체, 불리언을 저장할 때는 `--strict-json`을
  사용해야 합니다.

## 빠른 시작

반복 입력이 번거로우면 repo 루트의 `setup_pinchbench.sh` 로 자주 쓰는
설정 흐름을 한 번에 처리할 수 있습니다.

```bash
# 현재 관련 설정 보기
./setup_pinchbench.sh show

# Anthropic PinchBench preset 적용
./setup_pinchbench.sh anthropic \
  --model claude-sonnet-4-20250514 \
  --temperature 0.7 \
  --max-tokens 4096 \
  --fallback gemini

# 목표 메타데이터 기록
./setup_pinchbench.sh target \
  --score 0.85 \
  --suite all \
  --summary "match openclaw anthropic baseline"

# 현재 usage를 benchmark metadata에 기록
./setup_pinchbench.sh record-usage
```

## 1. 현재 설정 확인

전체 설정:

```bash
tizenclaw-cli config get
```

특정 값만 확인:

```bash
tizenclaw-cli config get active_backend
tizenclaw-cli config get backends.anthropic.model
tizenclaw-cli config get backends.gemini.model
tizenclaw-cli config get benchmark.pinchbench.target.score
```

## 2. Anthropic 설정

Anthropic을 기본 백엔드로 사용:

```bash
tizenclaw-cli config set active_backend anthropic
```

Anthropic 모델 설정:

```bash
tizenclaw-cli config set \
  backends.anthropic.model \
  claude-sonnet-4-20250514
```

Anthropic API 키 설정:

```bash
tizenclaw-cli config set \
  backends.anthropic.api_key \
  sk-ant-api03-...
```

Temperature 설정:

```bash
tizenclaw-cli config set \
  backends.anthropic.temperature \
  0.7 \
  --strict-json
```

최대 출력 토큰 설정:

```bash
tizenclaw-cli config set \
  backends.anthropic.max_tokens \
  4096 \
  --strict-json
```

## 3. Gemini 설정

Gemini를 기본 백엔드로 사용:

```bash
tizenclaw-cli config set active_backend gemini
```

Gemini 모델 설정:

```bash
tizenclaw-cli config set \
  backends.gemini.model \
  gemini-2.5-flash
```

Gemini API 키 설정:

```bash
tizenclaw-cli config set \
  backends.gemini.api_key \
  AIza...
```

Temperature 설정:

```bash
tizenclaw-cli config set \
  backends.gemini.temperature \
  0.7 \
  --strict-json
```

최대 출력 토큰 설정:

```bash
tizenclaw-cli config set \
  backends.gemini.max_tokens \
  4096 \
  --strict-json
```

## 4. fallback 백엔드 설정

Anthropic 우선, Gemini fallback:

```bash
tizenclaw-cli config set \
  fallback_backends \
  '["gemini"]' \
  --strict-json
```

Gemini 우선, Anthropic fallback:

```bash
tizenclaw-cli config set \
  fallback_backends \
  '["anthropic"]' \
  --strict-json
```

## 5. PinchBench용 실제 토큰 수 기록

PinchBench 비교 기록을 위해 실제 토큰 수를
`llm_config.json`에 남길 수 있습니다.

```bash
tizenclaw-cli config set \
  benchmark.pinchbench.actual_tokens.prompt \
  18234 \
  --strict-json

tizenclaw-cli config set \
  benchmark.pinchbench.actual_tokens.completion \
  4121 \
  --strict-json

tizenclaw-cli config set \
  benchmark.pinchbench.actual_tokens.total \
  22355 \
  --strict-json
```

한 번에 확인:

```bash
tizenclaw-cli config get benchmark.pinchbench.actual_tokens
```

## 6. PinchBench 목표 결과 기록

원하는 점수나 비교 목표도 같은 파일에 함께 저장할 수 있습니다.

목표 점수:

```bash
tizenclaw-cli config set \
  benchmark.pinchbench.target.score \
  0.85 \
  --strict-json
```

대상 suite:

```bash
tizenclaw-cli config set \
  benchmark.pinchbench.target.suite \
  all
```

비교 메모:

```bash
tizenclaw-cli config set \
  benchmark.pinchbench.target.summary \
  "match openclaw anthropic baseline"
```

확인:

```bash
tizenclaw-cli config get benchmark.pinchbench.target
```

## 7. 설정 반영

`active_backend`, `backends.*` 경로는 `config set` 시 daemon이 즉시
재적용합니다. 필요하면 수동으로 다시 리로드할 수도 있습니다.

```bash
tizenclaw-cli config reload
```

## 8. 캐시 토큰 사용량 확인

`--usage` 출력에는 누적 프롬프트/완성 토큰과 함께 캐시 관련 토큰도
포함됩니다.

```bash
tizenclaw-cli --usage
```

주요 필드:

- `prompt_tokens`: 전체 입력 토큰
- `completion_tokens`: 전체 출력 토큰
- `cache_creation_input_tokens`: 캐시 생성에 사용된 입력 토큰
- `cache_read_input_tokens`: 캐시 재사용으로 절감 추적되는 입력 토큰
- `total_requests`: 누적 요청 수

Anthropic 또는 Gemini에서 캐시가 실제로 동작한 뒤에는
`cache_creation_input_tokens` 또는 `cache_read_input_tokens` 값이
0보다 크게 보일 수 있습니다.

## 9. 설정 삭제

더 이상 필요 없는 benchmark 메모를 제거할 때:

```bash
tizenclaw-cli config unset benchmark.pinchbench.target.summary
```

## 10. OpenClaw/ZeroClaw 식 대응 예시

OpenClaw 식:

```bash
openclaw config set agents.defaults.thinkingDefault high
openclaw config set \
  'agents.defaults.models.anthropic/claude-sonnet-4-6.params.temperature' \
  0.7 \
  --strict-json
```

TizenClaw 식:

```bash
tizenclaw-cli config set active_backend anthropic
tizenclaw-cli config set \
  backends.anthropic.temperature \
  0.7 \
  --strict-json
tizenclaw-cli config set \
  backends.anthropic.max_tokens \
  4096 \
  --strict-json
```

ZeroClaw 식 TOML 편집:

```toml
default_provider = "anthropic"
default_model = "claude-sonnet-4-6"
default_temperature = 0.7
```

TizenClaw 식 CLI 설정:

```bash
tizenclaw-cli config set active_backend anthropic
tizenclaw-cli config set \
  backends.anthropic.model \
  claude-sonnet-4-20250514
tizenclaw-cli config set \
  backends.anthropic.temperature \
  0.7 \
  --strict-json
```

## 10. 권장 점검 순서

```bash
tizenclaw-cli config get active_backend
tizenclaw-cli config get backends.anthropic
tizenclaw-cli config get backends.gemini
tizenclaw-cli config get benchmark.pinchbench
```

이 출력이 기대값과 맞으면 PinchBench 실행 전 설정 확인이 끝난
상태로 보면 됩니다.
