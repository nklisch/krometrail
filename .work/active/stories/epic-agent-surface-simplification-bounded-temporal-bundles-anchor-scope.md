---
id: epic-agent-surface-simplification-bounded-temporal-bundles-anchor-scope
kind: story
stage: done
tags: [agent-ux, visual]
parent: epic-agent-surface-simplification-bounded-temporal-bundles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Select the anchor visual epoch before artifact work

Add the validated anchor/all bundle contract, remove frozen policy-version machinery, and narrow planned epochs before output counting, cache lookup, decoding, or generation. Acceptance evidence is focused core/schema, artifact-service, and bundle-service coverage proving default one-epoch work, deterministic nearest/earlier selection, explicit all, original descriptor indexes, and unchanged generic all-epoch generation.

## Implementation notes

- Execution capability: raised — the contract crosses core wire types, bundle orchestration, and artifact scheduling/order limits.
- Review weight: standard (autopilot caller); child story checkpoint, feature review owns the integrated pass.
- Files changed: core debug-bundle/artifact contracts and exports; artifact service and focused tests; bundle policy/service/tests; direct context call sites in progressive, video, MCP, and composition fixtures.
- Tests added/removed: added omitted-anchor/default and explicit-all request coverage, pre-output-limit selection with original descriptor indexes, and deterministic earlier-epoch tie coverage; deleted the frozen policy-version test.
- Simplification: removed the policy version constant, field, constructor argument, helper, imports, and version-specific comments; generic generation defaults directly to all epochs while the bundle supplies its narrower selection.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo check --workspace --all-targets --locked`
- `cargo test -p krometrail-core debug_bundle::tests --locked`
- `cargo test -p krometrail --bin krometrail anchor_epoch --locked`
- `cargo test -p krometrail --bin krometrail debug_bundle::tests::policy_tests --locked`
