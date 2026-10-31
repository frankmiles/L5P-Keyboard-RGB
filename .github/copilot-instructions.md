# Project-specific instructions

For architecture, device constraints, effect implementation patterns, input/cancellation behavior, custom-effect JSON, build prerequisites, and validation guidance, consult the root [PROJECT_KNOWLEDGE.md](../PROJECT_KNOWLEDGE.md). A workspace `SessionStart` hook also injects this reference at the start of agent sessions. Treat the current source code as authoritative when implementation details have changed.

Keep the four-zone, 12-byte RGB layout intact; use the existing keyboard driver API; preserve manager stop/shutdown semantics; and treat global keyboard input as optional.
