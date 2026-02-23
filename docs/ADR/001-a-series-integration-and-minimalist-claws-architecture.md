# ADR 001: a* Series Integration and Minimalist Claws Architecture

## Status
Proposed

## Context
`yuiclaw` is conceived as an OpenClaw-derivative "Claws" that aims to be extremely lightweight yet highly capable. The core challenge is: **Can we build a first-class AI assistant experience by only wrapping existing CLIs without implementing direct API calls?**

To achieve this, we leverage the `a*` series of specialized Rust modules.

## Decision
Adopt a modular architecture where `yuiclaw` acts as the glue between `amem`, `abeat`, `acomm`, and `acore`.

### 1. The Component Roles
- **`amem` (Memory):** The "Source of Truth" for long-term and mid-term context. It stores activity logs, preferences, and session transcripts in Markdown.
- **`abeat` (Heartbeat):** The "Proactive Trigger". It manages periodic tasks and monitors time-based events, triggering the agent without user input.
- **`acomm` (Communication):** The "Nervous System". It handles I/O via TUI (Ratatui) and external channels (Discord/Slack), providing real-time feedback.
- **`acore` (Core):** The "Brain". It manages the lifecycle of a task, selects the appropriate CLI (`gemini`, `claude`, etc.), and handles context injection/extraction.

### 2. Integration Workflow (The "Loop")
1. **Input:** `acomm` receives a prompt (via TUI or Discord).
2. **Mediation:** `acomm` forwards the request to `acore`.
3. **Recall:** `acore` queries `amem` to retrieve relevant past context and the "Owner Profile".
4. **Execution:** `acore` chooses the best-fit CLI tool and executes it as a subprocess, injecting the retrieved context.
5. **Real-time Output:** `acore` streams the CLI's `stdout` back to `acomm`, which updates the TUI or the Discord message in real-time.
6. **Persistence:** Once the CLI terminates, `acore` summarizes the turn and appends it to `amem` as a new activity record.
7. **Proactivity:** `abeat` periodically wakes up `acore` to perform "check-ups" defined in `HEARTBEAT.md`, utilizing the same loop.

### 3. Validation of the "No API" Constraint
- By delegating LLM intelligence to CLIs that are already authenticated on the host machine, `yuiclaw` avoids the complexity of API key management, rate-limiting logic (handled by the CLI/Provider), and prompt engineering for specific models.
- The "intelligence" of `yuiclaw` lies in **how it prepares the context** for these CLIs and **how it remembers the outcomes**.

## Consequences
- **Extremely Low Maintenance:** Updates to LLM capabilities are inherited automatically when the underlying CLIs are updated.
- **Tool-Agnostic Context:** The user can start a task with `claude` and finish it with `gemini` because the state is centrally managed in `amem`.
- **Privacy/Security:** All sensitive data (API keys) remains within the official CLIs' own configuration systems.
- **Modular Evolution:** Each `a*` component can be developed, tested, and replaced independently.
