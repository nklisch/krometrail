---
id: epic-agent-surface-simplification-persistence-recovery-report-structured-shutdown-recovery
kind: story
stage: done
tags: [browser, agent-ux, diagnostics]
parent: epic-agent-surface-simplification-persistence-recovery
depends_on: [epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Report structured closure quality, cause, and recovery

Replace the scalar degraded stop outcome with a structured closure result that separates released browser authority from evidence quality. Carry the first capture cause, typed failed phase, and recoverability-specific action through shutdown, concise status, MCP warnings, schemas, and current foundation docs.

## Acceptance evidence

- Clean managed and attached stops serialize as structured clean outcomes.
- A successfully closed browser with failed capture flush returns a degraded tool envelope with exact phase, capture cause, and recovery.
- `writer_usable` directs a new browser session; `writer_terminal` directs an MCP restart before a new session.
- Remaining managed process/profile authority still fails with `shutdown_incomplete`.
- End-to-end regression proves a fresh session persists after the post-rename sync failure in the same MCP process.

## Ordering

Depends on first-cause capture propagation and completes the externally visible recovery contract.

## Implementation notes

- Execution capability: high; the checkpoint spans capture teardown, browser authority release, typed recovery, MCP degradation, schemas, and privacy constraints.
- Review weight: standard (caller/project default).
- Files changed: structured stop contracts and port fixtures in `krometrail-core`; capture shutdown and session shutdown in `krometrail-cdp`; sink boundary classification in `krometrail-store`; lifecycle/status/warning/schema projection in `krometrail-mcp`; `docs/SPEC.md`, `docs/ARCHITECTURE.md`, and this story.
- Tests added/removed: added structured clean/degraded stop validation, writer-usable versus writer-terminal recovery, degraded MCP stop mapping, concise full-cause status, and current stop/capture schema coverage; scalar stop equality assertions were replaced with closure/quality assertions.
- Simplification: deleted scalar degraded stop variants, private duplicate shutdown quality, string failed phases, stage-only MCP reconstruction, and lossy boolean gap-persistence reporting; failure-rich status remains compact in memory by boxing only the internal optional cause.
- Discrepancies from design: no checked-in schema artifact directory exists; schema coverage is generated directly from the current Rust types in `krometrail-mcp/src/schema.rs`. The same-process recovery proof is layered at the private filesystem injection seam, capture seam, shutdown seam, and MCP projection seam because the production filesystem injector is intentionally not public.
- Adjacent issues parked: none.
- Verification: `cargo check --workspace --all-targets --locked`; strict clippy for core/store/CDP/MCP; focused core error/stop tests, store post-rename recovery, capture first-cause retention, shutdown recovery, MCP degraded stop/concise status/schema tests.
