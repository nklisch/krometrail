---
id: refactor-centralize-mcp-observation-unavailable-projection
kind: story
stage: implementing
tags: [refactor, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize MCP unavailable live-observation projection

## Brief

`crates/krometrail-mcp/src/response.rs:262-269` and `:329-336` independently map an unavailable `LiveObservation` page-operation/final-batch result to the same JSON shape, one warning, and no image. The available branch in both locations already delegates to the shared `project_live_observation` helper, but its surrounding `ObservationPart` mapping remains duplicated.

Extract one private helper for projecting `ObservationPart<LiveObservation>` with the requested image role and use it for page-operation and batch-final observations. Preserve the existing available JSON envelope, unavailable JSON envelope, warning count/order, image metadata/role, and invariant-error behavior. Do not broaden this into a general response-schema redesign or revisit the completed live-observation projection refactor.

**Source lens**: elimination / code smell

**Rationale**: removes the remaining duplicate mapping at the two response-boundary call sites and keeps degraded live-evidence semantics in one auditable place.

**Black-box classification**: pure refactor. Identical `BrowserOperationResult` values must produce identical structured content, MCP image content, status, warnings, errors, and serialization failures before and after the change.

## Acceptance criteria

- [ ] One private helper owns the `ObservationPart<LiveObservation>` available/unavailable projection used by both page operations and batch-final observations.
- [ ] `project_page_operation` and `project_batch` no longer duplicate the unavailable live-observation mapping.
- [ ] Available results retain their existing `PostAction` and `BatchFinal` image roles; unavailable results retain the same JSON shape, warning/error values, and degraded status.
- [ ] Existing `crates/krometrail-mcp/src/response.rs` unit tests pass without weakening or deleting assertions.
- [ ] `cargo fmt --all -- --check`, targeted MCP tests, and the locked workspace quality gates pass.

## Risk and rollback

**Risk**: Low. The duplicate branches are local and currently identical, but a changed generic/helper signature could accidentally alter warning propagation or image roles.

**Rollback**: Revert the implementation commit to restore the two inline `ObservationPart<LiveObservation>` matches.

## Discovery notes

- Scope: committed implementation paths touched from `e798b63` through committed `HEAD` `6e65586`, with direct focus on `crates/krometrail-mcp/src/response.rs`; uncommitted temporal-query work was excluded.
- Dispatch: direct-read only as required; no nested agents or peer review.
- Value: medium — a small boundary helper removes exact duplicate degraded-evidence mapping without changing the completed live-observation projection contract.
- Dependencies: none.
