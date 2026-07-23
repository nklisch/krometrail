---
id: idea-ax-overflow-opaque-failure
created: 2026-07-23
updated: 2026-07-23
tags: [browser-control, bug]
---

On pages whose accessibility tree exceeds what Chrome will serialize (repro:
`https://html.spec.whatwg.org/` one-page spec, content height ~2.25M CSS px),
`snapshot_page` and `query_page` fail opaquely instead of degrading or guiding
recovery. Found during the v1.6.0 full-surface shakedown.

Observed on 1.6.0:

- Both tools return only the bare error string "browser rejected or could not
  complete the page observation command". No correlation id, no structured
  recovery surfaced in the MCP error result (contrast: degraded action
  responses on the same page carry structured warnings with recovery text).
- Log shows `mcp.response.failed`, `failure_stage: operation`,
  `error_code: page_observation_failed` and nothing else — no CDP-level event
  records which command failed or why, so "AX tree too large" is
  indistinguishable from any other observation failure.
- The recovery text that does exist elsewhere for this code ("retry once; if it
  fails again, inspect browser compatibility and status") is wrong for this
  cause: retries always fail (each attempt burns ~10 s) and compatibility is
  fine.
- Post-action live observation on such a page also degrades persistently:
  scroll/click responses report both `page_observation_failed` (snapshot) and
  `screenshot_failed`, while a standalone `take_screenshot` can still succeed.
  Control itself (scroll, coordinate/CSS click, fragment navigation) keeps
  working.

Fix direction: classify the oversized/unserializable-AX acquisition failure
distinctly (bounded detail from the CDP error), return the structured
limit-style error with honest recovery (viewport-anchored/frame-scoped
targeting, snapshot alternatives, "this page exceeds browser AX serialization"),
and log a bounded CDP failure event so diagnostics can attribute the cause.
