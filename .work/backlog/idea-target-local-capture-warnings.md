---
id: idea-target-local-capture-warnings
created: 2026-07-17
updated: 2026-07-17
tags: [browser, agent-ux]
---

Target-specific v1.0.4 operations are marked degraded by a retained-capture failure on a different
page. After one Wikipedia target entered `failed` at `frame_envelope`, a newly created Hacker News
target remained in `capturing` and current inspection worked, yet `inspect_page` explicitly scoped to
the healthy Hacker News target returned `status: degraded` with a `capture_failed` warning whose
context named the failed Wikipedia target. Closing the failed target immediately restored successful,
warning-free inspection of Hacker News.

This makes a healthy page look unhealthy to an agent and obscures which target's temporal evidence is
actually unavailable. The repro used one managed session with two attached page targets; browser
status clearly distinguished the healthy and failed capture states.
