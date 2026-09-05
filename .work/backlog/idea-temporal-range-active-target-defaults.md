---
id: idea-temporal-range-active-target-defaults
created: 2026-08-15
updated: 2026-08-15
tags: [agent-ux, browser]
---

# Default Active Target & Session in Temporal Range Resolution

## The Finding
When invoking `resolve_temporal_range` using the `latest_interaction` or `session_time` anchor, the schema strictly requires explicit `session_id` and `target_id` fields.

In standard agent workflows where only one browser session and selected target exist, forcing the agent to query `browser_status` to extract UUIDs before requesting temporal evidence adds latency and failure surface.

## Fix Direction
1. In `crates/krometrail-mcp`, make `session_id` and `target_id` optional in `TemporalRangeAnchorRequestWire`.
2. When omitted:
   - Default `session_id` to the single active session (or error with clear advice if multiple sessions exist).
   - Default `target_id` to the currently selected active page target.
