---
id: epic-browser-interface-hardening-economical-projections-compact-temporal
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-browser-interface-hardening-economical-projections
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Compact temporal bundles independently

Add a temporal-specific compact/full response preference and ensure unrelated snapshot/page-state choices cannot expand default bundle output. Preserve the prior full projection under explicit opt-in.

## Implementation evidence

- Added additive `response.temporal` detail with a `compact` default and explicit `full` opt-in, including generated MCP schema coverage.
- Applied that one preference at both temporal response mappings. Snapshot and page-state preferences no longer select temporal detail; compact output preserves range/header, count summaries, artifact handles, warnings, and canonical resources.
- Added serialized-size coverage for a multi-frame compact bundle shape, response-preference independence coverage, and an MCP end-to-end `temporal: full` drill-down assertion.
- Verification: focused MCP response/schema/temporal-resource tests pass.
