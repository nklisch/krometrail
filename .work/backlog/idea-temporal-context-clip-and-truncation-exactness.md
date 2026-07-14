---
id: idea-temporal-context-clip-and-truncation-exactness
created: 2026-07-14
updated: 2026-07-14
tags: [browser, storage, agent-ux]
---

Tighten three lower-risk temporal-context reporting semantics before exposing them through MCP: decide whether capture gap summaries should follow the request's effective clip or the full resolved range and document the choice; distinguish scanned collection-gap count from total matched count; and probe one extra unavailable range so truncation warnings are exact rather than `len == limit` heuristics. Preserve current event selection, evidence records, and source-safe errors.
