---
id: idea-batch-schema-still-unknown
created: 2026-07-17
updated: 2026-07-17
tags: [bug, agent-ux]
---

Manual testing immediately after installing Krometrail v1.0.5 shows Codex still declares
`batch.steps` as `Array<unknown | unknown | ...>` with 19 unknown branches. The v1.0.5 MCP schema
publishes `items.anyOf` with concrete `operation` and `request` properties, and the shipped skill's
explicit example makes the tool usable, but the host-facing declaration has not materialized those
branch shapes. Revisit the projection design against the actual Codex declaration renderer rather
than treating the schema-level `anyOf` assertion as sufficient.
