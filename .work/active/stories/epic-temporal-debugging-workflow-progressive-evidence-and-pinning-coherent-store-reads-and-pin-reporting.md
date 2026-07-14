---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting
kind: story
stage: done
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Make Evidence Reads and Pins Coherent

## Checkpoint

Make `RecordingStore` the production `FrameSource` as well as artifact/retention authority. Implement source and artifact reads with metadata snapshot, out-of-gate bounded file I/O/hash validation, and final in-gate revalidation so eviction or session deletion during a read cannot return stale or partial bytes. Preserve authoritative cache-hit validation and distinguish missing from invalidated derived artifacts.

Upgrade exact pin/unpin/query reporting over the existing pins, segment links, segment metadata, and usage rows. Pin flushes open session segments and atomically proves the expected ordered `ResolvedRange` frames before linking segments; unpin is exact/idempotent and reports post-budget-enforcement overlap/availability truth. No schema, reader, cache, or pin ledger is added.

## Files

- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/src/index/{frames.rs,artifacts.rs,retention.rs}`
- `crates/krometrail-store/src/artifacts/mod.rs`
- `crates/krometrail-store/tests/{artifact_store.rs,retention_small_budget.rs}`
- `crates/krometrail-store/tests/progressive_evidence_store.rs` (new)

## Acceptance evidence

- Encoded frame/artifact reads return exact scoped rows/links/content or explicit `NotFound`/`EvidenceInvalidated`; source corruption remains `PersistenceFailed`.
- Controlled eviction/deletion races prove no already-invalidated payload or partial list escapes and no mutation gate spans file reads or hashing.
- Frame order, scope, metadata, hashes, lengths, and source links are revalidated; list hashes reuse the bounded encoded read path without a persisted frame-hash column.
- Pin flush/revalidation prevents empty or partly stale pins, while returned segment bounds expose segment-granular overreach.
- Overlap, repeated pin/unpin, exact unpin, coalescing, concurrent eviction/deletion, paused-budget recovery, and final availability/status are deterministic.
- Pin tables protect source segments only. Existing artifact deletion and v5 browser-event eviction remain independent and session deletion removes every authority.

## Ordering

Depends on the public contracts checkpoint. Current reference geometry can proceed afterward, but progressive service composition waits for both store and browser seams.

## Implementation notes

- Execution capability: highest, retained from the caller because coherent weak handles, destructive retention races, source-safe corruption classification, and exact pin truth are future public evidence guarantees. Dispatch remained direct-read and single-owner; no nested agent was used.
- Review weight: standard from the caller; review is not applicable at this child-story checkpoint and remains feature-scoped.
- Files changed: `.work/active/stories/epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting.md`; `crates/krometrail-core/src/lib.rs`; `crates/krometrail-core/src/ports/{artifacts.rs,frames.rs,mod.rs,retention.rs}`; `crates/krometrail-core/src/recording/frame.rs`; `crates/krometrail-store/src/recording.rs`; `crates/krometrail-store/src/index/{frames.rs,retention.rs,segments.rs}`; `crates/krometrail-store/tests/{artifact_store.rs,progressive_evidence_store.rs,retention_small_budget.rs}`.
- Core contract correction: the just-landed contracts could describe progressive values but the existing ports could not request scoped artifact reads, bounded source handle reads, or resolved-range pin state. Those operations were added to the existing `ArtifactStore`, `FrameSource`, and `RetentionStore` ports with source-compatible defaults; `RecordingStore` is the only implementation that overrides the progressive operations. `EncodedFrame::encoded_bytes` clones its existing `Arc<[u8]>` so request payloads do not copy bytes.
- Coherent reads: `RecordingStore` snapshots full frame address+metadata or artifact row+source links+source frame snapshots under its mutation gate, releases the gate for segment/artifact file reads and all SHA-256/length/provenance checks, then reacquires it for exact final snapshot/session/source revalidation. Progressive source list/fetch enforce request/capture order and count/item/total ceilings atomically; cache lookup and scoped artifact reads preserve validated hit behavior while separating missing, derived invalidation, and source corruption.
- Pinning: resolved pin flushes the session, validates the exact ordered in-range frame set against sealed source segments in the same immediate transaction that inserts the exact pin and links intersecting segments, then enforces budget and reports final exact/overlap/availability truth. Query/unpin derive actual protected segment bounds/bytes, true coalesced unions, global distinct pinned usage, source-only scope, and final retention status from existing schema-v5 rows. Exact overlapping pins remain independent and repeated pin/unpin exposes `changed=false`.
- Tests added/updated: deterministic source/artifact read pauses prove eviction, session deletion, append progress, metadata changes, and artifact-link hash changes cannot escape stale payloads and that the mutation gate is not held over file work; corruption remains `PersistenceFailed`; direct artifact corruption is `Invalidated`; source order/scope/hash/length and byte boundaries are exact; open-segment flush, stale/partial/unavailable expected frames, segment overreach, overlap/idempotence/exact unpin, paused-budget recovery, source-only artifact/event behavior, and session deletion are covered.
- Simplification: no reader, cache, pin ledger, frame-hash column, schema migration, read lease, or payload table was added. Existing artifact provenance validation, segment files, pin tables, usage rows, and deletion authority remain the only stores of truth. Test coordination hooks compile only in store unit tests.
- Discrepancies from design: root composition is explicitly outside this checkpoint, so existing range/artifact composition continues to reference `SqliteIndex` until the designed service/root story switches production wiring. `SqliteIndex` does not implement the new progressive methods; `RecordingStore` is the sole coherent progressive source/artifact/retention authority. No other design deviation.
- Adjacent issues parked: none.

## Verification evidence

- `rustup run 1.85.0 cargo fmt --all -- --check` — passed.
- `rustup run 1.85.0 cargo check -p krometrail-core -p krometrail-store --all-targets --locked` — passed.
- `rustup run 1.85.0 cargo test -p krometrail-core -p krometrail-store --all-targets --locked` — passed, 213 tests across core and store targets.
- `rustup run 1.85.0 cargo clippy -p krometrail-core -p krometrail-store --all-targets --locked -- -D warnings` — passed.
- `rustup run 1.85.0 cargo check --workspace --all-targets --locked` — passed as the required reverse-dependency check.
- Existing schema inventory/migration tests pass unchanged; no schema file or migration was modified.
