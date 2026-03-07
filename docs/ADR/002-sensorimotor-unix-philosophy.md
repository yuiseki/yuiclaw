# ADR 002: Sensorimotor Architecture — Human Body as a Model for AI Agents

## Status

Proposed

## Context

As the `a*` module family grows to include perception (ahear, asee), expression (asay, ashow), and actuation (adesk), we need a unifying architectural philosophy to guide how these modules are designed and how they interact.

The existing PoC experiments (`tmp/_trash/tauri-god-mode`, `webcam_ollama_vision`, `whispercpp-listen`) suffer from a common pathology: **a single process owns multiple unrelated responsibilities**. `voice_command_loop.py`, for example, simultaneously handles microphone capture, VAD, whisper inference, VOICEVOX TTS, and VacuumTube control. This violates the UNIX principle of "do one thing well" and makes the system fragile and hard to extend.

## Decision

Adopt the **Sensorimotor Architecture**: model the AI agent after the human body, where each `a*` module corresponds to a specific biological organ with a single, well-defined responsibility. All inter-organ communication passes through the nervous system (`acomm`).

### The Body Mapping

```
┌───────────────────────────────────────────────────────────────────┐
│                         AI Agent Body                             │
│                                                                   │
│  Sensory Organs (Input)                                           │
│    ahear  ──  耳  (Ears)    ── Audio → Text                      │
│    asee   ──  目  (Eyes)    ── Video → Text                      │
│                                                                   │
│  Central Nervous System                                           │
│    acomm  ── 神経系 (Nervous System) ── Signal Transmission      │
│    acore  ── 脳   (Brain)            ── Cognition & Decision     │
│    amem   ── 記憶 (Memory)           ── Hippocampus              │
│                                                                   │
│  Autonomic Nervous System                                         │
│    abeat  ── 心拍 (Heartbeat) ── Background Rhythmic Processes   │
│                                                                   │
│  Motor Organs (Output)                                            │
│    asay   ──  口  (Mouth)    ── Text → Voice                     │
│    ashow  ──  顔  (Face)     ── Text/Events → Visual Display     │
│    adesk  ── 手足 (Limbs)    ── Commands → Desktop Actions       │
└───────────────────────────────────────────────────────────────────┘
```

### Core Principles

#### 1. Single Responsibility per Organ
Each module does exactly one thing:
- `ahear` transcribes audio. It does **not** parse commands.
- `asay` vocalizes text. It does **not** decide what to say.
- `asee` describes what it sees. It does **not** act on the description.
- `adesk` executes desktop actions. It does **not** generate them.
- `ashow` renders what it is told to render. It does **not** decide content.

#### 2. acomm is the Only Nervous System
Organs do **not** call each other directly. All inter-organ signals pass through `acomm`'s Unix Domain Socket event bus. This is analogous to how biological nerves carry signals without understanding their content.

```
ahear → (publishes RecognizedSpeech event) → acomm
acomm → (routes to acore)
acore → (publishes SpeakRequest event) → acomm
acomm → (routes to asay)
asay  → (vocalizes)
```

No organ imports or directly spawns another organ.

#### 3. stdin/stdout as the Universal Interface
Each organ must be usable standalone via UNIX pipes:

```sh
# Pipe hearing output directly to speaking (without acomm, for testing)
ahear | asay

# Pipe webcam vision to memory
asee --camera 0 | amem keep --kind activity

# Chain perception to desktop action
ahear | acore | adesk
```

This ensures each module is independently testable and composable.

#### 4. abeat as the Autonomic Nervous System
`abeat` manages processes that must run rhythmically and without conscious intervention: periodic vision captures, heartbeat health checks, scheduled agent tasks. It starts/stops organs as needed but does not own their logic.

#### 5. ashow Absorbs the TUI
The `acomm-tui` (currently embedded in `acomm`) is a visual rendering concern and should eventually migrate into `ashow` (or a dedicated `atui` sub-component of `ashow`). `acomm` itself should remain a pure signal bus with no UI logic.

### Data Flow (Full Loop)

```
Microphone
  └─▶ [ahear]  ──(RecognizedSpeech)──▶ [acomm] ──▶ [acore]
                                                       │
                                              (queries [amem])
                                                       │
                                              (generates response)
                                                       │
                                          ┌────────────┴────────────┐
                                          ▼                         ▼
                                    [acomm]──▶[asay]          [acomm]──▶[ashow]
                                    (voice out)               (caption display)
                                          │
                                          ▼
                                    [acomm]──▶[adesk]
                                    (desktop action)

Webcam
  └─▶ [asee]  ──(VisualContext)──▶ [acomm] ──▶ [acore]  (same loop)

[abeat] ──(ticks)──▶ starts/stops [ahear], [asee], checks [amem]
```

## Consequences

### Positive
- **Composability:** Any organ can be replaced, upgraded, or mocked independently.
- **Testability:** Each organ can be tested with simple stdin/stdout pipes.
- **Clarity:** The responsibility of every module is obvious from its biological analogy.
- **Resilience:** Failure of one organ (e.g., `asee` crashes) does not bring down others.
- **PoC → Production path:** Each existing PoC maps cleanly to one organ, extracting only its relevant responsibility.

### Negative / Risks
- **acomm becomes a bottleneck:** All inter-organ communication passes through a single socket. Care must be taken to keep the event bus low-latency.
- **Migration effort:** Existing PoC code mixes responsibilities and must be surgically separated before it can be promoted to a module.

## Migration Map: PoC → Module

| PoC File | Extract Into | Discard Into |
|---|---|---|
| `whispercpp-listen/listen_only_whisper_server.py` | **ahear** (mic + VAD + whisper) | — |
| `whispercpp-listen/voice_command_loop.py` VOICEVOX part | **asay** | — |
| `whispercpp-listen/voice_command_loop.py` VacuumTube/command part | **adesk** | — |
| `webcam_ollama_vision/describe_webcam_with_ollama.py` | **asee** | daemon mgmt → abeat |
| `tauri-caption-overlay-poc/` | **ashow** | — |
| `tmp/_trash/tauri-god-mode/god_mode.sh` orchestration | **abeat** job definitions | archived PoC reference only |

## Related ADRs

- [ADR 001: a* Series Integration and Minimalist Claws Architecture](./001-a-series-integration-and-minimalist-claws-architecture.md)
