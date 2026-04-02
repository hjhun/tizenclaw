# Capability: Session & Persistent Tool Executor
- **Goal:** Format agent tool conversations as markdown and manage background pipeline processes for tool interaction.
- **Inputs:** Agent prompts, IPC Commands targeting background tool sessions.
- **Outputs:** Markdown log `session-<hash>.md`. Stdout/Stderr streams from the Tool Executor daemon.
- **Resource Impact:** File sizing limits must be strictly verified before serialization to avoid parsing OOM. Tool executor uses standard pipes.
