---
id: idea-expose-browser-event-detail
created: 2026-07-18
updated: 2026-07-18
tags: [browser]
---

Released `query_browser_events` advertises its arguments as two opaque `{ [key: string]: unknown }` alternatives, so an agent cannot discover the chronological request shape from the MCP schema. After reconstructing the request from source and successfully calling it with a temporal `range_handle`, the default response projected away the actual `events` array and returned only counts, range, and capture-quality summary. The tool describes itself as chronological browser-event detail and its request type intentionally has no compact selection, so the callable schema and returned detail surface do not currently fulfill that contract.
