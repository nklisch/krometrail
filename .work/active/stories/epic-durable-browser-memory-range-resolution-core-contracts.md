---
id: epic-durable-browser-memory-range-resolution-core-contracts
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-range-resolution
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Define core temporal range contracts

## Checkpoint

Add the single core registry and response contract for temporal range resolution in `crates/krometrail-core/src/timeline/range.rs`, export it through `timeline/mod.rs` and `lib.rs`, and keep the core free of store/CDP/MCP/temporal-vision dependencies.

## Required contract

- `TemporalRangeAnchorKind::ALL` and stable names cover session time, wall clock, interaction, latest interaction, navigation, marker, and source-frame anchors.
- `TemporalRangeAnchor` carries one `AnchorScope` and exact anchor inputs.
- `RangeResolutionOptions` carries `RetentionPolicy`, `CaptureGapPolicy`, and the effective implicit interaction window.
- `ResolvedRange` carries session, target, requested range, resolved retained range, ordered frame IDs, related interaction/navigation/marker IDs, full gap records, retention warnings, and the effective options.

## Acceptance evidence

- [x] Constructor tests reject empty frame lists, duplicate IDs, unordered ranges, partial retention without warnings, and resolved ranges outside the request.
- [x] Registry tests prove stable names, `ALL`, Serde, and reverse lookup stay in one declaration.
- [x] Boundary tests prove zero-length ranges are valid, endpoint inclusivity is explicit, and duration-to-nanosecond conversion failures are not lossy.
- [x] The core port/source scanner still finds no adapter/runtime dependencies in core.

## Implementation

Added the registry-backed `TemporalRangeAnchor`/`ResolvedRange` contracts, checked interaction windows, retention and gap policies, and constructor invariants in `krometrail-core`. Added domain-only `TimelineAnchorSource` and `InteractionAnchorSource` ports and extended catalog/frame ports for the next checkpoints. Core tests pass: `cargo test -p krometrail-core --all-targets --locked` (66 tests).
