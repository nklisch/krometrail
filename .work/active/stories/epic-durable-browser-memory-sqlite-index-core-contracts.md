---
id: epic-durable-browser-memory-sqlite-index-core-contracts
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-sqlite-index
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Core Metadata Ports, Observation Names, and Lossless Gaps

## Checkpoint

Publish the infrastructure-free contracts the SQLite adapter needs before SQL lands. Extend the existing macro-backed observation declaration so one entry generates Serde name, `ObservationKind::ALL`, `as_str`, and reverse lookup. Add `ObservedTime` to `CaptureGap`; CDP samples the existing monotonic clock at declaration and gap coalescing keeps the maximum declaration time.

Add focused object-safe ports at `crates/krometrail-core/src/ports/{catalog,frames,gaps}.rs` with the exact `RecordingCatalog`, `FrameSource`, and `CaptureGapStore` signatures in the parent feature. `TimelineStore::range` becomes explicitly inclusive and deterministic. Re-export through `ports/mod.rs` and `lib.rs`; mechanically update every `CaptureGap::new` caller and fake.

## Ordering

First checkpoint. It has no sibling dependency and publishes the names, values, and ports consumed by every later story.

## Acceptance evidence

- One observation declaration generates all stable names and reverse lookup; exhaustive tests fail when a variant lacks either.
- Gaps retain exact range, declaration time, reason, estimate, and detail; impossible declaration ordering rejects at constructors and Serde boundaries.
- CDP gap declaration uses its injected clock and coalescing retains the maximum `ObservedTime` without changing range/estimate semantics.
- Catalog, gap, and frame ports expose only core/std values, preserve requested id order/range semantics, and remain object-safe.
- Existing runtime/transport/database source guards and locked workspace quality gates pass.

## Implementation notes

- The observation macro now owns each stable name alongside its payload contract and generates Serde names, `ALL`, `as_str`, and reverse lookup from that declaration.
- `CaptureGap` stores declaration `ObservedTime`, validates that the declared interval has already occurred, and preserves the maximum declaration time when bounded CDP gap entries coalesce.
- Added the focused domain-only `RecordingCatalog`, `CaptureGapStore`, and `FrameSource` ports and documented inclusive deterministic timeline ranges.
- Mechanical capture/test callers now sample or provide declaration time; no browser-control behavior was changed.
- Verification: `cargo check -p krometrail-core -p krometrail-cdp -p krometrail-store --all-targets --locked`; 224 core/CDP tests passed.
