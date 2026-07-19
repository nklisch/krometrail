---
id: agent-visual-response-surface-compact-identities
kind: story
stage: done
tags: [agent-ux, browser, storage, testing]
parent: agent-visual-response-surface
depends_on: [agent-visual-response-surface-followup-contracts]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Compact repeated temporal and sanitized URL identities

Use one compact resolved-range projection in concise results and one validated lowercase-hex digest representation for sanitized URLs, with an intentional current-store schema bump and no compatibility path.

## Implementation notes

- Execution capability: GPT-5.6, high reasoning; the story changes one current persisted identity and several projections of the same canonical range.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-core/src/browser/privacy.rs`, `crates/krometrail-store/src/index/schema.rs`, `crates/krometrail-mcp/src/response.rs`.
- Tests added/updated: lowercase 64-character sanitized path digest, incompatible schema-version rejection including v6, stored browser-event roundtrip, and bounded 29-frame concise range with full ordered-ID preservation.
- Simplification: reused `Sha256Digest` instead of maintaining raw digest-byte serialization; centralized concise range mapping in `compact_resolved_range`.
- Discrepancies from design: `crates/krometrail-store/src/index/browser_events.rs` required no production edit because it already serializes/deserializes the canonical `SanitizedUrl`; its integration test verifies the new representation.
- Adjacent issues parked: none.
- Verification: focused `krometrail-core` privacy test; `krometrail-store` schema and browser-event suites; full `krometrail-mcp` response tests.
- Workflow deviation: `.work/bin/work-view` is an x86-64 Linux binary unavailable on this macOS host; dependency readiness was verified from direct item frontmatter and commit `6b83e06`.
