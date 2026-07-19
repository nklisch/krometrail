---
id: idea-runtime-focus-escalation
created: 2026-07-18
updated: 2026-07-18
tags: [browser]
---

In a managed `focus: preserve` session, another browser surface made Krometrail's selected page hidden. Pointer work correctly failed as `target_hidden` without stealing focus, but the only recovery was to stop the entire managed session and start a new one with `focus: foreground`; this discarded the active session, download cursor, and immediate retained context. Consider a deliberate runtime foreground or focus-policy escalation that preserves the current managed session and remains explicit to the agent and user.
