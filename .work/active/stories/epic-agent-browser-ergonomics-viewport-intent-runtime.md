---
id: epic-agent-browser-ergonomics-viewport-intent-runtime
kind: story
stage: done
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics-viewport-intent
depends_on: [epic-agent-browser-ergonomics-viewport-intent-contract]
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Integrate presets with the viewport lifecycle

Materialize presets before the existing apply/observe/commit boundary, decode effective layout and visual geometry plus viewport-meta presence, return bounded guidance, and qualify responsive/mobile behavior through generated MCP schema, real Chrome, and the plugin skill.

## Acceptance evidence

- Scripted lifecycle tests prove preset/custom command equivalence, rollback, reconnect, clear, and target isolation.
- A valid mobile visual/layout mismatch succeeds with specific guidance instead of being corrected or failed.
- Real Chrome and MCP tests verify the two intent classes, provenance, and unchanged custom behavior.

## Ordering

Depends on `epic-agent-browser-ergonomics-viewport-intent-contract`; completes the feature's externally usable slice.

## Implementation notes

- Execution capability: direct inline implementation across the existing viewport lifecycle, CDP observation, MCP projection/schema, real-browser qualification, and plugin skill.
- Review weight: standard child story; no independent story review required before the parent feature review.
- Changed `crates/krometrail-cdp/src/control/viewport.rs`, `crates/krometrail-cdp/src/session/{mod.rs,operations.rs}`, `crates/krometrail-cdp/tests/verified_interactions.rs`, `crates/krometrail-mcp/src/{response.rs,schema.rs}`, and `plugin/skills/krometrail/SKILL.md`.
- Materializes presets before the established apply/observe/supervisor-commit/capture-commit boundary; reconnect, rollback, and supervisor state continue carrying only `Option<ViewportMetrics>`.
- Independently decodes positive finite visual and layout CSS geometry plus a boolean viewport-meta fact. Acknowledgement remains bound to requested visual size, DPR, and touch; valid layout divergence becomes guidance rather than failure.
- MCP returns materialization provenance, effective visual/layout geometry, and bounded guidance. The schema publishes three modes, all five presets, and unchanged custom metric bounds.
- The skill leads with `responsive_small` as the low-friction default, expands through larger responsive presets, reserves mobile presets for mobile/touch behavior, preserves custom metrics for bespoke geometry, and explains the no-user-agent guarantee.
- Real Chrome qualified responsive-small equal visual/layout geometry and exact screenshot size, mobile-phone no-meta layout divergence with specific guidance, navigation persistence, clear, and second-target isolation.
- Simplification: one metrics authority and one `set_viewport` tool remain; no preset persistence, UA override, device catalog, or alternate emulation state machine was added.
- Discrepancies and adjacent findings: none.

## Verification

- `cargo fmt --all`
- `cargo test -p krometrail-core browser::viewport::tests --locked`
- `cargo test -p krometrail-cdp control::viewport::tests --locked`
- `cargo test -p krometrail-cdp --lib session_set_clear_and_rollback_fence_capture_geometry_transactions --locked`
- `cargo test -p krometrail-mcp --lib published_viewport_schema_matches_runtime_bounds --locked`
- `cargo test -p krometrail-mcp --lib viewport_projection_publishes_materialization_geometry_and_bounded_guidance --locked`
- `cargo check --workspace --all-targets --locked`
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --features cdpkit-transport --test verified_interactions opt_in_real_chrome_qualifies_viewport_presets_guidance_and_target_isolation --locked -- --nocapture`
