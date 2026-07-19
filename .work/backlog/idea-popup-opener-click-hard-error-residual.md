---
id: idea-popup-opener-click-hard-error-residual
created: 2026-07-19
updated: 2026-07-19
tags: [bug, browser, agent-ux]
---

Residual from `feature-window-lifecycle-integrity` (live MCP qualification on the
final batch build, 2026-07-19, session b453e799): a click on a
`window.open`-triggering button now correctly opens the popup, which commits
navigation and is adopted with `opener_target_id` (`wait_for_page` matches — the
headline fix works), but the opener's click response is still the hard error
"click failed: browser rejected or could not complete the page observation
command" even though the input demonstrably dispatched. The feature's
observation-degradation unit covers the post-dispatch
observation-unavailable path in `control/interaction.rs`, yet this path — the
post-action observation's transport command failing while the popup steals
focus/compositing (`transport_error` in `control/mod.rs`) — still propagates as
an operation error, dropping the interaction record.

Repro: managed foreground session on a page whose button calls
`window.open(url, name, 'width=,height=')`; `click` on that button; observe the
hard error while the popup appears and is adopted.

Fix direction: route the post-action observation's transport-command failure on
a dispatched interaction through the same degraded-with-record shape the
feature added for observation-unavailable, keeping genuine dispatch failures
hard. One deterministic double: dispatched click + observation transport
command_failed → degraded response with interaction record and diagnostics.
