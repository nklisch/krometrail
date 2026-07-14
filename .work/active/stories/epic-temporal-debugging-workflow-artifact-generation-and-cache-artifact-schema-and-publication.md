---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication
kind: story
stage: done
tags: [visual, storage]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-contracts-and-cache-identity]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Build Artifact Schema and Atomic Publication

## Checkpoint

Turn the reserved test-only artifact tables into the one production artifact store/cache authority. Add strict staging/ready publication state, exact manifest/output hashes, ordered source links with encoded-content hashes, cache metadata, and usage reservation. Implement ready-only validated lookup, file-before-ready atomic publication, corruption invalidation, and startup convergence through the existing retention/deletion/recovery authorities. Neither core nor root receives a filesystem path.

This checkpoint exclusively owns metadata schema migration **v4**. It purges legacy derived artifact rows/usage that cannot satisfy the new exact contracts, leaves source evidence untouched, and lets recovery remove orphan files. The sibling browser-event feature must depend on this story and add **schema v5 or later**; it must not edit or claim v4.

## Files

- `crates/krometrail-store/src/index/schema_v4.rs` (new; exclusive ownership)
- `crates/krometrail-store/src/index/{migrations.rs,artifacts.rs,mod.rs,retention.rs,deletion.rs,maintenance.rs}`
- `crates/krometrail-store/src/artifacts/{mod.rs,files.rs,recovery.rs}` (new)
- `crates/krometrail-store/src/{recording.rs,recovery.rs,lib.rs}`
- `crates/krometrail-store/tests/{artifact_store.rs,artifact_recovery.rs}` (new)

## Acceptance evidence

- Fresh and v3 databases migrate transactionally to contiguous v4; future versions refuse and failed migration rolls back.
- Equal cache-key publication races converge on one ready artifact; cache readers never observe staging.
- Temp write, file fsync, rename, directory fsync, and ready transaction failpoints all converge after reopen.
- Hit validation covers exact stored manifest bytes/hash, output bytes/hash, metadata, cache/source fingerprints, ordered retained source links, and authoritative typed-manifest invariants.
- Corrupt/missing entries are deletion-journaled and become misses; usage never undercounts physical artifact bytes.
- No file writing/hashing occurs under a SQLite transaction or the recording mutation gate.

## Ordering

Depends on the core contracts/cache identity checkpoint. The frame adapter and generation service consume this store boundary; browser-event migration work chains after this story as v5+.

## Implementation notes

- Execution capability: highest; durable SQLite/filesystem publication, recovery, retention, and deletion fencing are one integrity boundary.
- Review weight: standard from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: `crates/krometrail-store/{Cargo.toml,src/artifacts/{mod.rs,files.rs,recovery.rs},src/index/{artifacts.rs,schema_v4.rs,migrations.rs,mod.rs,maintenance.rs},src/{recording.rs,lib.rs},tests/{artifact_store.rs,sqlite_schema.rs}}` and `Cargo.lock`.
- Tests added/updated: v3→v4 legacy-derived purge with source identities retained; real artifact publication/validated lookup/corruption invalidation/exact usage; concurrent equal-key convergence; durable staging finalization, orphan cleanup, corrupt-ready invalidation, and idempotent reopen; file-phase failpoint state tests. The obsolete test-only direct artifact-row SQL fixture was removed because v4's production port now covers the retention seam without bypassing invariants.
- Verification: `cargo fmt --all`; `cargo check -p krometrail-store --all-targets`; all store all-target tests (79 passed); store all-target Clippy with `-D warnings` (green).
- Simplification: v4 rebuilds and purges the reserved test-era artifact tables instead of carrying a compatibility path; one `RecordingStore` implements the path-free artifact authority and reuses the deletion journal, usage ledger, mutation gate, and source links.
- Publication semantics: one cache-key lock serializes local publishers; staging metadata/source links/exact usage commit first, a bounded file worker performs temp write + file sync + rename + directory sync outside the mutation gate, and finalization revalidates session/source/cache state before ready visibility. Lookup snapshots metadata, reads/hashes source and artifact bytes outside the gate, then revalidates before returning.
- Recovery/retention semantics: deletion journals resume before artifact reconciliation; valid durable staging rows finalize, invalid rows use the deletion journal, managed temp/final orphans are removed, and usage is reconciled idempotently. Source maintenance purges linked derived metadata before frame rows; normal eviction journals linked staging/ready artifact files before source segments. Session deletion marks/cancels and drains active publications before acquiring the mutation gate.
- Discrepancies from design: recovery reporting is logged during `RecordingStore` construction rather than extending the segment-only public `RecoveryReport`; artifact publication uses a dedicated bounded single-thread file worker and per-key mutex rather than exposing failpoints through public ports. Physical crash states and ready visibility remain as designed.
- Adjacent issues parked: none.
