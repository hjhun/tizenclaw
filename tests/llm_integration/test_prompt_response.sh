#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# LLM Integration Tests — Prompt & Response
# Validates basic LLM prompt/response, multi-language, and context.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "LLM Integration: Prompt & Response"

# ── L1: Basic Korean prompt ──────────────────────────────────────
section "L1" "Basic Korean prompt"
OUT=$(tc_cli "안녕하세요, 짧게 한 문장으로 답해주세요")
assert_not_empty "Korean prompt returns response" "$OUT"

# ── L2: Basic English prompt ─────────────────────────────────────
section "L2" "Basic English prompt"
OUT=$(tc_cli "Hello, respond with one short sentence please")
assert_not_empty "English prompt returns response" "$OUT"

# ── L3: Device-related question ──────────────────────────────────
section "L3" "Device context question"
OUT=$(tc_cli "이 디바이스는 무슨 기기인가요? 간단히 알려주세요")
assert_not_empty "Device question returns response" "$OUT"

# ── L4: Math question (reasoning) ────────────────────────────────
section "L4" "Simple reasoning"
OUT=$(tc_cli "What is 15 + 27? Just give me the number")
assert_not_empty "Math question returns response" "$OUT"
assert_contains "Correct answer" "$OUT" "42"

# ── L5: Response quality ─────────────────────────────────────────
section "L5" "Response is not an error"
OUT=$(tc_cli "What is your name?")
assert_not_empty "Response not empty" "$OUT"
assert_not_contains "No error in response" "$OUT" "error.*occurred\|failed to\|connection refused"

# ── L6: Long prompt handling ─────────────────────────────────────
section "L6" "Long prompt handling"
LONG_PROMPT="Please summarize the following text in one sentence: The quick brown fox jumps over the lazy dog. This is a test of handling longer inputs with multiple sentences. The system should process this without issues."
OUT=$(tc_cli "$LONG_PROMPT")
assert_not_empty "Long prompt returns response" "$OUT"

suite_end
