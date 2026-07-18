---
id: idea-clarify-unscoped-exact-text-wait
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux]
---

An exploratory plugin run used `wait` with a text condition for `Hello World!`, `match_mode: exact`, and no locator. The page visibly contained an exact `Hello World!` heading, but the wait timed out because unscoped exact matching compared against the full observed page text (`observed_length: 122`). Repeating with `match_mode: contains` succeeded immediately. Preserve the current behavior if intentional, but make the unscoped semantics harder for agents to misread through schema wording, recovery guidance, or a more explicit scope contract. Diagnostic correlation: `4050e515-4569-4ba5-bc22-6936fcd56131`.
