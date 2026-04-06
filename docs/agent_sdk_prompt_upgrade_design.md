# Agent SDK Prompt Upgrade Design

## Design Goal

Translate the review conclusions into low-risk runtime improvements while
keeping the existing Rust agent loop intact. The design keeps Claude
Agent SDK adoption open, but implements only the pieces that improve the
current runtime immediately: prompt policy separation and OpenClaw skill
hub reuse.

## Architecture Summary

### 1. Prompt Policy Layer

- Extend `SystemPromptBuilder` with an explicit prompt mode:
  `full` or `minimal`.
- Add a reasoning policy layer that can be selected explicitly or
  resolved automatically from the active backend.
- Keep the stable instructions inside the built system prompt, but leave
  runtime facts such as working directory and current time in dynamic
  overlay messages to preserve cache stability.

### 2. Backend-Aware Reasoning

- Hosted backends such as Anthropic, OpenAI, Gemini, and xAI should use
  native/private reasoning guidance and plain final responses by
  default.
- Local backends such as Ollama should continue to tolerate tagged
  reasoning when that improves agent-loop reliability.
- Final response extraction should no longer depend solely on
  `<final>...</final>` being present; it must also strip any `<think>`
  block and return the visible answer safely.

### 3. OpenClaw Skill Hub Compatibility

- Add a canonical `workspace/skill-hubs` directory under platform paths.
- Treat each child directory inside that hub area as a skill root and
  scan it using the existing OpenClaw-compatible `SKILL.md` scanner.
- Keep the existing registered external path mechanism so
  `tizenclaw-cli register skill <path>` can mount an arbitrary OpenClaw
  hub without copying its contents.

## Safety And Runtime Constraints

- No new FFI boundary is introduced.
- `Send + Sync` ownership remains unchanged because the new policy data
  is derived from config reads and stack-local builder state.
- The current `libloading` strategy is unaffected.
- No local `cargo build`, `cargo test`, `cargo check`, or `cargo clippy`
  is needed; validation remains in the managed deploy flow.

## Validation Plan

1. Use `tizenclaw-cli config set` to update prompt policy values.
2. Use `tizenclaw-cli register skill` to attach an external OpenClaw
   skill hub.
3. Run `./deploy.sh -a x86_64`.
4. Re-check deploy-time tests and service health.
5. Use `tizenclaw-cli list registrations` and a smoke prompt to confirm
   the runtime observes the new configuration path.
