---
id: refactor-centralize-mcp-live-observation-projection
kind: story
stage: review
tags: [refactor, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize MCP live-observation projection for screenshot-bearing control results

## Brief

`crates/krometrail-mcp/src/response.rs` repeats the same live-observation mapping pattern in four places: `ObserveLive` at `response.rs:208-219`, interaction results at `response.rs:238-252`, page-operation results at `response.rs:272-297`, and batch final-observation handling at `response.rs:344-357`. Each copy independently unwraps `project_live_observation`, applies degradation warnings, and conditionally appends an `EncodedMcpImage` with a role. The logic is already coupled: recent fixes to batch degradation semantics landed beside this repeated projection code, and future tweaks to warning/image behavior would have to touch four branches.

Extract one private helper that turns a `LiveObservation` or `ObservationPart<LiveObservation>` into the JSON projection, warning list, and optional encoded image, with caller-supplied image role/step context. Use it from the existing observe-live, page, interaction, and batch branches without changing any response schema, summary text, status/error mapping, or image-role assignments.

**Source lens**: missing abstraction / pattern drift

**Rationale**: makes one helper authoritative for MCP live-evidence projection so future response-semantics fixes do not drift across four branches.

**Black-box classification**: pure refactor. Structured response JSON, degraded/failed status rules, warning contents, screenshot metadata, image MIME handling, text summaries, and caller-visible stable errors remain unchanged.

## Acceptance criteria

- [ ] One private helper in `crates/krometrail-mcp/src/response.rs` owns the repeated live-observation-to-response projection used by observe-live, page operations, interaction operations, and batch final observation.
- [ ] Existing image roles (`live_observation`, `post_action`, `batch_final`) and step-index behavior remain byte-for-byte unchanged.
- [ ] Existing degradation and failure behavior, including batch final-observation-only degradation, remains unchanged.
- [ ] `cargo fmt --all -- --check`, focused MCP response tests, and `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings` pass.

## Risk and rollback

**Risk**: Low. The work stays inside one response-mapping file, but it sits on a public MCP boundary, so image-role or warning drift would be user-visible.

**Rollback**: Revert the refactor commit to restore the four inline branches.

## Implementation notes

- Execution capability: baseline inline ownership; the change is one private-helper extraction in one file with complete existing response tests.
- Review weight: standard from autopilot; as a standalone story this uses bounded inline review and no independent reviewer.
- `project_live_observation` now owns JSON/warning/screenshot projection plus caller-supplied `ImageRole` and optional step index, returning an `EncodedMcpImage` directly.
- Observe-live, interactions, page operations, and batch final observation all reuse that helper while retaining their caller-specific availability wrappers, anchors, status/failure rules, and image roles.
- No response schema, summary text, warning/error ordering, MIME logic, or batch degradation behavior changed.
- Verification passed `cargo fmt --all -- --check`, all 9 `krometrail-mcp` tests, and MCP all-target Clippy with warnings denied.
