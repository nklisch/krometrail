---
id: gate-tests-wait-schema-field-guidance
kind: story
stage: done
tags: [testing, agent-ux]
parent: null
depends_on: []
release_binding: 1.0.3
gate_origin: tests
created: 2026-07-17
updated: 2026-07-17
---

# Bind exact-text guidance to the published wait fields

## Priority

Medium, promoted for immediate implementation by the operator's fix-all release request.

## Value evidence

Item: `story-clarify-unscoped-exact-text-wait`. The schema regression must prove the locator and match-mode fields themselves retain full-body, complete-scope, and `contains` recovery guidance.

## Suggested test

Locate the tagged text condition branch in the published wait schema and assert each description on its owning field. Protect the parallel shipped-skill wording in plugin static qualification.

## Implementation notes

- Execution capability: inline; two focused contract assertions.
- Review weight: standard by project default.
- Files changed: `crates/krometrail-mcp/src/schema.rs`, `tests/plugin-static.sh`.
- Tests added: tagged text-branch field descriptions and exact shipped-skill recovery wording.
- Simplification: replaced whole-schema substring checks with owning-field assertions.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Review (2026-07-17)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Bounded inline standalone-story review. Verified tagged-variant lookup, field ownership, complete-scope wording, `contains` recovery, and shipped-skill static protection.
