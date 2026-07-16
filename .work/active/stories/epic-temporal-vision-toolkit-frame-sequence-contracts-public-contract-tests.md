---
id: epic-temporal-vision-toolkit-frame-sequence-contracts-public-contract-tests
kind: story
stage: done
tags: [visual, testing]
parent: epic-temporal-vision-toolkit-frame-sequence-contracts
depends_on: [epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Prove the Public Contract and Crate Independence

## Checkpoint

Add `crates/temporal-vision/tests/contracts.rs` as a consumer-level proof over the completed public surface. Build a deterministic 2x2 synthetic RGBA sequence with local distinct newtype IDs, tied frame timestamps, tied caller markers, a declared gap, a half-frame region, and a binary mask. Exercise borrowed construction without pixel copying, explicit owned conversion, ordered iteration, complete manifest projection, repeated deterministic JSON serialization, and validated JSON round-trip.

Keep malformed cases focused on load-bearing risks: RGBA length, frame duplication/order, marker/gap range and order, gap overlap, region checked bounds, mask padding, finite parameters, selected-ID subsequence order, computed counts, and malformed persisted manifests. Colocated generated loops cover stable enum registries. Do not test trivial getters, derives, debug output, or private layout.

## Acceptance evidence

- The integration test imports only `temporal_vision` plus test support and uses its own strongly typed IDs; no Krometrail, browser, DOM, CDP, MCP, storage, filesystem, or runtime type shapes the API.
- Identical inputs produce byte-identical serialized manifests and preserve all tied ordering.
- Constructor and deserialization tests reject each listed invariant violation without weakening assertions to current behavior.
- `cargo tree -p temporal-vision --edges normal` contains no Krometrail crate, CDP, MCP, Tokio, filesystem adapter, or image codec.
- `cargo fmt --all -- --check`, `cargo check -p temporal-vision --all-targets --locked`, `cargo test -p temporal-vision --all-targets --locked`, and `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` pass.

## Ordering

Depends on `epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest`. This is the final checkpoint and validates the complete feature as one public contract.

## Implementation notes

- Execution capability: highest/raised (caller-selected) for the stable downstream consumer seam.
- Review weight: standard (caller/autopilot).
- Files changed: `crates/temporal-vision/tests/contracts.rs`.
- Tests added: browser-free typed-ID borrowed/owned sequence and complete manifest round trip; deterministic JSON; malformed frame, order, annotation, gap, geometry, mask, number, selected-subsequence, and persisted-count cases.
- Simplification: one 2x2 synthetic fixture exercises the integrated contract without browser fixtures, image codecs, UUID libraries, or runtime support.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: focused check/test/clippy all passed locked (12 tests); normal dependency tree contains only Serde and thiserror.
