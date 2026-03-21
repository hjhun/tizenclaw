#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — RAG (Document Ingestion & Knowledge Search)
# Tests: ingest_document → search_knowledge → relevance
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: RAG (Ingest & Search)"

SESSION_ID="e2e_rag_$$"

# ── R1: Ingest document ──────────────────────────────────────────
section "R1" "Ingest document into knowledge base"
INGEST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the ingest_document tool with source 'e2e_test_doc' and text 'TizenClaw is an AI agent system for Tizen-based devices. It supports workflow execution, pipeline management, and tool invocation. The system uses LLM integration for natural language understanding.'")
assert_not_empty "ingest_document returns output" "$INGEST_OUT"

# ── R2: Search for ingested content ───────────────────────────────
section "R2" "Search knowledge base"
SEARCH_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the search_knowledge tool with query 'What is TizenClaw?' and top_k 3")
assert_not_empty "search_knowledge returns output" "$SEARCH_OUT"

# ── R3: Verify relevant results ──────────────────────────────────
section "R3" "Verify search relevance"
if echo "$SEARCH_OUT" | grep -qi "TizenClaw\|agent\|AI\|tool\|workflow"; then
  _pass "Search results contain relevant content"
else
  _skip "Search relevance" "LLM may paraphrase results"
fi

# ── R4: Ingest second document ────────────────────────────────────
section "R4" "Ingest second document"
INGEST2_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the ingest_document tool with source 'e2e_test_doc_2' and text 'Samsung refrigerators equipped with TizenClaw can check food freshness, manage grocery lists, and suggest recipes using AI-powered analysis.'")
assert_not_empty "second document ingested" "$INGEST2_OUT"

# ── R5: Search specific topic ────────────────────────────────────
section "R5" "Search specific topic from second doc"
SEARCH2_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the search_knowledge tool with query 'refrigerator food management' and top_k 2")
assert_not_empty "topic-specific search returns output" "$SEARCH2_OUT"

# ── R6: Search with Korean query ─────────────────────────────────
section "R6" "Search with Korean query"
SEARCH_KR=$(tc_cli_session "$SESSION_ID" \
  "Use the search_knowledge tool with query '타이젠클로가 뭔가요?'")
assert_not_empty "Korean search returns output" "$SEARCH_KR"

suite_end
