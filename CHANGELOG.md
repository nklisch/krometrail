# Changelog

## v1.0.6 — 2026-07-17

### Features

- Added a managed-launch `focus` policy: `foreground` preserves existing behavior, while `preserve`
  keeps Chrome visible for observers without Krometrail activating the window or switching tabs.
- Made preserve-mode page creation explicitly open background tabs and reject hidden-tab pointer
  work before input, while retaining logical target selection for non-pointer control and evidence.

### Fixes

- Made explicit batch targets apply to targetless child steps instead of accidentally following a
  different selected page.
- Flattened batch-step MCP schemas into concrete operation/request objects that agent hosts can
  render and construct reliably.
- Compacted batch child results by removing repeated live observations while retaining per-step
  outcomes, timing, errors, screenshots, and the final observation.
- Reapplied and independently verified acknowledged viewport overrides before returning
  post-navigation evidence, avoiding native-layout screenshots after mobile navigation.

### Agent guidance

- Updated the shipped Krometrail skill with exact focus-preserving launch guidance, visible-tab
  recording limits, background page creation behavior, and foreground recovery advice.

## v1.0.5 — 2026-07-17

### Fixes

- Kept capture healthy through transient navigation-time geometry reads and mobile page-scale resets by replaying the declared target override before independently recommitting geometry, while preserving fenced transition gaps and fail-closed behavior for persistent failures.
- Let pointer activation visibility settle within its existing deadline, avoiding false `target_hidden` failures without weakening the persistent-hidden error contract.
- Scoped page-operation capture warnings to the operated target so one failed tab no longer degrades healthy pages, while retaining session-wide health on browser-scoped results.
- Published concrete tagged batch-step branches in the MCP schema so agent hosts can construct `batch` requests from the advertised tool contract.

### Agent guidance

- Added an explicit batch-step request shape and example to the shipped Krometrail skill.

## v1.0.4 — 2026-07-17

### Fixes

- Made screencast frame geometry truthful across dynamic viewport changes, native resize, navigation, zoom, reconnect, and viewport rollback by fencing ambiguous transitions and recording exact capture gaps instead of attaching stale viewport or DPR provenance.
- Made compact temporal bundles recover cleanly from never-captured ranges, preserve eviction semantics, fit high-DPI default artifacts within the fixed combined memory budget, and expose canonical manifest resources for progressive full-provenance reads.
- Bounded automatic post-action page snapshots while preserving explicit full observations, and deduplicated equivalent warnings across logs and MCP responses so agent feedback stays concise and actionable.

### Agent guidance

- Updated the shipped Krometrail skill and activation checks to teach `manifest_uri` drill-down and verify the temporal artifact-manifest resource alongside image and source-frame resources.

## v1.0.3 — 2026-07-17

### Fixes

- Made mobile viewport overrides reliable on pages without a viewport meta tag by applying and replaying an explicit mobile page scale, while preserving target isolation, navigation persistence, independent effective-metric verification, and native clear behavior.
- Clarified in the generated MCP schema and shipped Krometrail skill that unscoped exact-text waits compare the complete document-body text.
- Prevented automatically selected temporal-bundle markers outside an exact resolved range from invalidating artifact generation.
- Clamped storyboard render anchors to each visual epoch's retained source-frame interval while preserving the original semantic range anchor as provenance.

## v1.0.2 — 2026-07-17

### Fixes

- Hardened managed-browser discovery, pointer activation, dialog and fill races, target reference lifetimes, truthful shutdown, and viewport restoration across navigation and reconnect.
- Added durable privacy-safe diagnostic logging and expanded the shipped agent skill with log-driven troubleshooting and issue-reporting guidance.
- Added a companion issue-reporting skill that prepares reproducible, redacted Krometrail reports for authenticated GitHub submission.

## v1.0.1 — 2026-07-16

### Plugin distribution

- Published first-party native Claude Code and Codex marketplaces with one portable Krometrail evidence-literacy skill, MCP configuration, isolated install/remove qualification, and remote pointers from the nklisch skills catalogs.
- Added package-owned Claude Code and Codex MCP launchers that install the exact release coupled to the plugin version into private per-user data on first activation.
- Added exact checksum and executable-identity verification, HTTPS redirect allowlisting, bounded downloads, symlink and ownership defenses, atomic publication, and warm offline startup.
- Kept standalone binary installation independent while making native plugin installation sufficient for MCP activation.
- Derived native manifests, first-party catalog entries, and the launcher version marker atomically from Cargo's sole release version authority.
- Added ordinary-CI hermetic bootstrap fault and Linux/macOS x64/arm64 mapping coverage, plus isolated native Claude/Codex install, activation, tool/resource discovery, update-layout, and removal qualification.

### Fixes

- Corrected the Linux arm64 release workflow to install `cross` from its real `v0.2.5` upstream tag.

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
