# Long-Term Memory Architecture

## 1. Goal of Cognitive Action
The long-term memory system aims to provide the TizenClaw autonomous agent with cross-session persistence of facts, user preferences, learned skills, and device profiles. By injecting these stored insights into the system prompt, the agent effectively "remembers" context across system restarts and time, preventing redundant information gathering and improving response quality.

## 2. Input Requirements
- **LLM Output (Post-Response)**: The agent's full textual responses are passed to a distinct LLM processing pipeline.
- **User Explicit Commands**: Users can directly issue conversational commands to remember ("기억해") or forget ("잊어버려") information.
- **Session History**: Active conversation history is maintained separately but provides immediate context if needed.

## 3. Output Generated
- **Memory Markdown Files**: Persistent textual files stored in `/opt/usr/share/tizenclaw/memory/` (e.g., `facts.md`, `preferences.md`).
- **System Prompt Injection**: Formatted Markdown blocks seamlessly injected into the base System Prompt before LLM completion queries.
- **SQLite Index (Optional/Internal)**: Maintained strictly for quick searching in the future, while Markdown remains the source of truth.

## 4. Resource Mitigation (Embedded Devices)
- **Lazy Extraction**: To avoid slowing down the active conversational pipeline, the extraction LLM call runs asynchronously (Daemon Sub-task / Background One-shot worker). 
- **Rule-based & Dedicated LLM Extraction**: Uses a heavily constrained, cheap/small LLM specifically structured for extraction, minimizing tokenizer overhead.
- **Static Inject**: Markdown content is directly appended to the system prompt text instead of requiring expensive dynamic queries per user interaction, relying on `ContextEngine` to manage overall token budgets.
