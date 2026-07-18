---
id: epic-agent-browser-ergonomics-response-projections-route-integration
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-response-projections
depends_on: [epic-agent-browser-ergonomics-response-projections-projector]
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Projection routing, concise status, and agent guidance

## Checkpoint

Wire the shared response preference through browser and temporal routes, add the additive concise `browser_status` request, honor explicit diagnostic omission without weakening structured failures, regenerate schema fixtures, and teach agents to request the cheapest sufficient projection.

## Acceptance evidence

- Stdio integration covers legacy, compact, full, omit, concise status, invalid preference, and failed/degraded diagnostic behavior.
- Concise status retains capture loss/failure and retention-pressure facts while excluding compatibility and timing detail.
- Plugin instructions include economical request examples and explicit drill-down guidance.

## Ordering

Depends on `epic-agent-browser-ergonomics-response-projections-projector`; it consumes that single contract rather than introducing route-local variants.

## Implementation notes

- Execution capability: direct inline implementation; registry routing, status projection, server diagnostics, and plugin guidance shared one response-preference contract and benefited from one owner.
- Review weight: standard (project default); review applies at the integrated feature boundary, not this child checkpoint.
- Files changed: `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/schema.rs`, `crates/krometrail-mcp/src/server.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`.
- Tests added/removed: added concise-status failure/retention coverage, economical-default stdio coverage, validated diagnostic omission, and explicit inline temporal bundle coverage; removed none.
- Simplification: every eligible route uses one schema decorator and one request splitter; status is a serialization projection of `BrowserStatus`, and diagnostic omission is decided once at the server boundary.
- Discrepancies from design: at the user's explicit direction, omitted response preferences now select compact/no-inline output and omitted status detail selects concise output; explicit `legacy`, `full`, and `inline` preserve expansion paths. Existing generated operation roots remain open for stable request compatibility while the nested response preference is closed.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-mcp --locked`
- `cargo check -p krometrail-mcp --all-targets --locked`
- `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings`
