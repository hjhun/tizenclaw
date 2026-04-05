#!/bin/bash
#
# TizenClaw PinchBench setup helper
#
# Wraps the most common `tizenclaw-cli config ...` flows from
# docs/tizenclaw_cli_pinchbench_guide_ko.md into a simple shell interface.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEFAULT_CLI="${HOME}/.tizenclaw/bin/tizenclaw-cli"

DRY_RUN=false
CLI_BIN=""

usage() {
  cat <<'EOF'
Usage:
  ./setup_pinchbench.sh [global-options] <command> [command-options]

Global options:
  --dry-run              Print planned changes without applying them
  --cli <path>           Use a specific tizenclaw-cli binary
  -h, --help             Show this help

Commands:
  show                   Print the relevant PinchBench config snapshot
  anthropic              Configure Anthropic for PinchBench
  gemini                 Configure Gemini for PinchBench
  target                 Set benchmark.pinchbench.target fields
  clear-target           Remove benchmark.pinchbench.target fields
  record-usage           Store current --usage totals into benchmark metadata
  clear-usage            Remove benchmark.pinchbench.actual_tokens fields
  reload                 Trigger tizenclaw-cli config reload

anthropic options:
  --model <id>           Default: claude-sonnet-4-20250514
  --api-key <key>        Optional; keeps existing key if omitted
  --temperature <n>      Default: 0.7
  --max-tokens <n>       Default: 4096
  --fallback <name>      Default: none; use gemini or none

gemini options:
  --model <id>           Default: gemini-2.5-flash
  --api-key <key>        Optional; keeps existing key if omitted
  --temperature <n>      Default: 0.7
  --max-tokens <n>       Default: 4096
  --fallback <name>      Default: none; use anthropic or none

target options:
  --score <n>            Optional benchmark target score
  --suite <name>         Optional suite label
  --summary <text>       Optional comparison memo

record-usage options:
  -s, --session <id>     Read usage for a specific session

Examples:
  ./setup_pinchbench.sh show
  ./setup_pinchbench.sh anthropic --model claude-sonnet-4-20250514 --fallback gemini
  ./setup_pinchbench.sh gemini --api-key 'AIza...'
  ./setup_pinchbench.sh target --score 0.85 --suite all --summary 'baseline run'
  ./setup_pinchbench.sh record-usage -s pinchbench_trace_02
EOF
}

fail() {
  echo "[FAIL] $*" >&2
  exit 1
}

log() {
  echo "[INFO] $*"
}

resolve_cli() {
  if [ -n "${CLI_BIN}" ]; then
    [ -x "${CLI_BIN}" ] || fail "CLI not executable: ${CLI_BIN}"
    return
  fi

  if command -v tizenclaw-cli >/dev/null 2>&1; then
    CLI_BIN="$(command -v tizenclaw-cli)"
    return
  fi

  [ -x "${DEFAULT_CLI}" ] || fail "tizenclaw-cli not found; use --cli <path>"
  CLI_BIN="${DEFAULT_CLI}"
}

mask_value() {
  local path="$1"
  local value="$2"
  if [[ "${path}" == *".api_key" ]]; then
    echo "<redacted>"
  else
    echo "${value}"
  fi
}

run_cli() {
  if [ "${DRY_RUN}" = true ]; then
    printf '[DRY-RUN]'
    for arg in "$@"; do
      printf ' %q' "${arg}"
    done
    printf '\n'
    return 0
  fi
  "${CLI_BIN}" "$@"
}

config_set_string() {
  local path="$1"
  local value="$2"
  log "config set ${path} $(mask_value "${path}" "${value}")"
  run_cli config set "${path}" "${value}" >/dev/null
}

config_set_json() {
  local path="$1"
  local value="$2"
  log "config set ${path} ${value}"
  run_cli config set "${path}" "${value}" --strict-json >/dev/null
}

config_unset() {
  local path="$1"
  log "config unset ${path}"
  run_cli config unset "${path}" >/dev/null
}

show_snapshot() {
  run_cli config get active_backend
  run_cli config get backends.anthropic
  run_cli config get backends.gemini
  run_cli config get benchmark.pinchbench
}

cmd_anthropic() {
  local model="claude-sonnet-4-20250514"
  local api_key=""
  local temperature="0.7"
  local max_tokens="4096"
  local fallback="none"

  while [ $# -gt 0 ]; do
    case "$1" in
      --model) model="${2:?}"; shift 2 ;;
      --api-key) api_key="${2:?}"; shift 2 ;;
      --temperature) temperature="${2:?}"; shift 2 ;;
      --max-tokens) max_tokens="${2:?}"; shift 2 ;;
      --fallback) fallback="${2:?}"; shift 2 ;;
      *) fail "Unknown anthropic option: $1" ;;
    esac
  done

  config_set_string "active_backend" "anthropic"
  config_set_string "backends.anthropic.model" "${model}"
  [ -z "${api_key}" ] || config_set_string "backends.anthropic.api_key" "${api_key}"
  config_set_json "backends.anthropic.temperature" "${temperature}"
  config_set_json "backends.anthropic.max_tokens" "${max_tokens}"

  case "${fallback}" in
    none) config_set_json "fallback_backends" '[]' ;;
    gemini) config_set_json "fallback_backends" '["gemini"]' ;;
    *) fail "Unsupported anthropic fallback: ${fallback}" ;;
  esac

  log "Anthropic PinchBench preset applied"
}

cmd_gemini() {
  local model="gemini-2.5-flash"
  local api_key=""
  local temperature="0.7"
  local max_tokens="4096"
  local fallback="none"

  while [ $# -gt 0 ]; do
    case "$1" in
      --model) model="${2:?}"; shift 2 ;;
      --api-key) api_key="${2:?}"; shift 2 ;;
      --temperature) temperature="${2:?}"; shift 2 ;;
      --max-tokens) max_tokens="${2:?}"; shift 2 ;;
      --fallback) fallback="${2:?}"; shift 2 ;;
      *) fail "Unknown gemini option: $1" ;;
    esac
  done

  config_set_string "active_backend" "gemini"
  config_set_string "backends.gemini.model" "${model}"
  [ -z "${api_key}" ] || config_set_string "backends.gemini.api_key" "${api_key}"
  config_set_json "backends.gemini.temperature" "${temperature}"
  config_set_json "backends.gemini.max_tokens" "${max_tokens}"

  case "${fallback}" in
    none) config_set_json "fallback_backends" '[]' ;;
    anthropic) config_set_json "fallback_backends" '["anthropic"]' ;;
    *) fail "Unsupported gemini fallback: ${fallback}" ;;
  esac

  log "Gemini PinchBench preset applied"
}

cmd_target() {
  local score=""
  local suite=""
  local summary=""

  while [ $# -gt 0 ]; do
    case "$1" in
      --score) score="${2:?}"; shift 2 ;;
      --suite) suite="${2:?}"; shift 2 ;;
      --summary) summary="${2:?}"; shift 2 ;;
      *) fail "Unknown target option: $1" ;;
    esac
  done

  [ -z "${score}" ] || config_set_json "benchmark.pinchbench.target.score" "${score}"
  [ -z "${suite}" ] || config_set_string "benchmark.pinchbench.target.suite" "${suite}"
  [ -z "${summary}" ] || config_set_string "benchmark.pinchbench.target.summary" "${summary}"

  log "PinchBench target metadata updated"
}

cmd_clear_target() {
  config_unset "benchmark.pinchbench.target.score" || true
  config_unset "benchmark.pinchbench.target.suite" || true
  config_unset "benchmark.pinchbench.target.summary" || true
  log "PinchBench target metadata cleared"
}

cmd_record_usage() {
  local session_id=""
  while [ $# -gt 0 ]; do
    case "$1" in
      -s|--session) session_id="${2:?}"; shift 2 ;;
      *) fail "Unknown record-usage option: $1" ;;
    esac
  done

  local usage_json
  if [ -n "${session_id}" ]; then
    usage_json="$("${CLI_BIN}" -s "${session_id}" --usage)"
  else
    usage_json="$("${CLI_BIN}" --usage)"
  fi

  local prompt_tokens
  local completion_tokens
  local total_tokens
  prompt_tokens="$(printf '%s' "${usage_json}" | python3 -c 'import json,sys; data=json.load(sys.stdin); usage=data.get("usage", data); print(int(usage.get("prompt_tokens", data.get("prompt_tokens", 0))))')"
  completion_tokens="$(printf '%s' "${usage_json}" | python3 -c 'import json,sys; data=json.load(sys.stdin); usage=data.get("usage", data); print(int(usage.get("completion_tokens", data.get("completion_tokens", 0))))')"
  total_tokens="$((prompt_tokens + completion_tokens))"

  config_set_json "benchmark.pinchbench.actual_tokens.prompt" "${prompt_tokens}"
  config_set_json "benchmark.pinchbench.actual_tokens.completion" "${completion_tokens}"
  config_set_json "benchmark.pinchbench.actual_tokens.total" "${total_tokens}"
  log "PinchBench actual_tokens updated from current usage"
}

cmd_clear_usage() {
  config_unset "benchmark.pinchbench.actual_tokens.prompt" || true
  config_unset "benchmark.pinchbench.actual_tokens.completion" || true
  config_unset "benchmark.pinchbench.actual_tokens.total" || true
  log "PinchBench actual_tokens cleared"
}

cmd_reload() {
  log "config reload"
  run_cli config reload >/dev/null
}

main() {
  local args=()
  while [ $# -gt 0 ]; do
    case "$1" in
      --dry-run) DRY_RUN=true; shift ;;
      --cli) CLI_BIN="${2:?}"; shift 2 ;;
      -h|--help) usage; exit 0 ;;
      *) args+=("$1"); shift ;;
    esac
  done

  [ "${#args[@]}" -gt 0 ] || { usage; exit 1; }

  resolve_cli

  local command="${args[0]}"
  case "${command}" in
    show) show_snapshot ;;
    anthropic) cmd_anthropic "${args[@]:1}" ;;
    gemini) cmd_gemini "${args[@]:1}" ;;
    target) cmd_target "${args[@]:1}" ;;
    clear-target) cmd_clear_target ;;
    record-usage) cmd_record_usage "${args[@]:1}" ;;
    clear-usage) cmd_clear_usage ;;
    reload) cmd_reload ;;
    *) fail "Unknown command: ${command}" ;;
  esac
}

main "$@"
