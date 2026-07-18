---
id: epic-browser-interface-hardening-economical-projections-bound-snapshots
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

# Bound automatic snapshots

Tighten compact automatic snapshots to 48 nodes and 12 KiB while preserving actionable ancestry, full opt-in, and exact omission accounting. Verification is the MCP serialized-budget regression described by the parent feature.

## Implementation evidence

- Reduced the shared automatic snapshot limits in `crates/krometrail-mcp/src/response.rs` to 48 nodes and 12 KiB of serialized node JSON; explicit `full` projection remains unchanged.
- The live-observation regression now asserts the literal node and serialized-byte ceilings, validates exact omission accounting, and reconstructs the selected preorder snapshot to protect parent validity.
- Verification: `cargo test -p krometrail-mcp automatic_live_observations_bound_complex_snapshots_with_exact_omissions --locked`.
