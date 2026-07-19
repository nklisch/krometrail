---
id: browser-runtime-manual-test-hardening-frame-query
kind: story
stage: implementing
tags: [browser, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Acquire only semantic data required by the query

Role/name queries use the frame-scoped accessibility tree without an unnecessary full DOM snapshot. DOM-dependent query variants retain exact semantic acquisition and completeness checks, with bounded diagnostics that identify which selected acquisition exceeded its limit.
