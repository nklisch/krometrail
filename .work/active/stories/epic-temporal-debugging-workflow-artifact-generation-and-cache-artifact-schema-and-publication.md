---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication
kind: story
stage: implementing
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