# Changelog

## v1.0.0 — 2026-07-15

### Features

- Added the Rust browser-control runtime with managed Chrome launch/attach, target lifecycle, page selection, live screenshots, structured snapshots, verified interactions, waits, batches, dialogs, uploads, and cancellation-aware execution.
- Added the MCP 2025-06-18 stdio server with generated lifecycle/control contracts, temporal investigation tools, progressive evidence operations, browser-event queries, and retained-evidence resources.
- Added continuous CDP screencast capture with prompt acknowledgements, bounded ingestion, explicit capture gaps, reconnect supervision, and immutable per-session capture cadence through `every_nth_frame`.
- Added durable local browser memory backed by SQLite metadata and append-only frame segments, including temporal range resolution, retention budgets, pinning, eviction, deletion, recovery, and corruption handling.
- Added temporal visual evidence generation: normalization and measurements, storyboards, before/during/after views, region filmstrips, difference maps, motion history, artifact caching, progressive source access, and temporal debug bundles.
- Added browser-event context and provenance across capture, interactions, temporal queries, generated artifacts, and MCP responses.
- Added deterministic temporal-evaluation contracts, defect/control fixtures, evidence conditions, structured scoring, live qualification composition, platform-lane contracts, and privacy-safe manifests.
- Added stable Linux, macOS, and Windows release assets, checksums, build-provenance attestations, installer validation, and release-tag identity checks.

### Performance

- Specialized opaque row-major normalization, reducing normalization time by about 58% and improving the measured 120-frame production-policy artifact workload by about 20% without changing output bytes or hashes.
- Added reproducible release benchmarks for normalization, pair-classification, overlapping temporal queries, cache states, allocation volume, RSS, and stage timing.
- Preserved bounded capture headroom while rejecting pair-sharing and cross-query intermediate-cache experiments that did not meet their complete performance and correctness gates.

### Security

- Pinned privileged release workflow actions to reviewed immutable commit SHAs.
- Replaced managed Chrome's preallocated debugging port with Chrome-owned ephemeral port discovery and validated profile-scoped endpoint handoff.
- Added core validation for initial browser URLs while retaining local `http`, `https`, `file`, `about`, and `data` navigation.
- Enforced owner-only Unix permissions for Krometrail-created evidence, index, segment, and managed-profile paths.
- Prevented recovery from following segment-shaped symlinks or mutating their targets.

### Fixes

- Hardened browser reconnect, target-generation, acknowledgement ordering, cancellation, capture-gap, retention, artifact single-flight, and recovery invariants.
- Preserved exact per-frame retention evidence, mixed cache-state reporting, source/session/observed clock separation, marker identity, and canonical manifest status aggregation.
- Added successful end-to-end temporal MCP artifact-resource coverage and non-default capture-cadence provenance coverage.

### Documentation and internal structure

- Updated README, runtime/MCP/configuration references, architecture, evidence non-claims, and Rust CDP guidance for the implemented 1.0 surface.
- Codified project patterns for generated registries, validated wire contracts, injected ports, deterministic reducers, bounded-loss accounting, layered CDP qualification, ordered SQL migrations, and canonical schema artifacts.
