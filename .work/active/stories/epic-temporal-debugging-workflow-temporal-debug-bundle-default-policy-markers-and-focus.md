---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus
kind: story
stage: done
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-contracts-and-manifest-trace
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Materialize Default Evidence, Markers, and Focus Times

## Checkpoint

Implement the pure, versioned `temporal-debug-bundle-v1` policy: exact default generator requests, bounded privacy-safe marker assembly from existing timeline/interaction sources, and deterministic major-change focus extraction from the typed storyboard trace. This checkpoint adds no generation, event correlation, marker store, or bundle orchestration loop.

## Files

- `crates/krometrail-core/src/ports/{timeline.rs,range.rs,mod.rs}`
- `crates/krometrail-core/src/debug_bundle.rs`
- `crates/krometrail-store/src/index/{timeline.rs,interactions.rs}`
- `crates/krometrail-store/src/recording.rs`
- `src/debug_bundle/{mod.rs,policy.rs,markers.rs,focus.rs}` (new)
- focused tests beside these modules and `crates/krometrail-store/tests/sqlite_timeline.rs`

## Policy

- Storyboard: anchor from `ResolvedAnchor.effective_time`, 8 tiles, noise floor 512, no crop, black background, existing exact `FitLimits`, `1920 × 2048`, 16 MiB, and orientation included by default/omittable only through `OrientationPolicy`.
- Difference map: epoch-local first reference, normalized frequency, existing spectral palette, implicit range-quarter repeat separation, noise floor 512, same normalization, black canvas, `8192 × 8192`, 64 MiB. Use `AllowPartial`; request no motion history, filmstrip, region, comparison, or inferred output.
- Add bounded kind-filtered `TimelineStore::selected_range(TimelineRangeQuery) -> TimelineRangeSlice`; bundle uses only interaction/navigation/marker kinds with a 1024-row source cap, exact filtered `matched_count`, and explicit truncation.
- Preserve caller markers exactly (maximum 64, kind 64 UTF-8 bytes, label 160), interaction dispatch/operation identity, explicit navigation identity/time, and generic marker identity/time. Use exact caller presentation where supplied; otherwise emit the explicit non-sensitive `Marker <uuid>` fallback and warning.
- Preserve caller and resolved-anchor markers first, select remaining candidates by exact anchor distance/class/time/ID to the existing 256-marker artifact cap, then present chronologically.
- Extract at most 16 focus times from available storyboard summaries and selected major-change reasons only; deduplicate/tie-break as designed and sort chronologically for `TemporalContextRequest`.

## Acceptance evidence

- Default and orientation-omitted effective requests are byte-stable and contain only the approved existing artifact kinds/parameters/limits.
- Kind-filtered timeline SQL returns an exact matched count plus at most the requested rows, excludes browser-event rows, preserves generic timeline order, and requires no migration or alternate table.
- Interaction records/anchors, navigation observations, caller markers, and generic markers produce exact deterministic `ArtifactMarker` values with mandatory inclusion, equal-time order, and honest 64/256/1024 truncation.
- Labels contain only typed IDs and operation stable names; persisted secret, locator, page-text, URL, filename, key, and parameter sentinels do not appear.
- Focus extraction covers unchanged/change/gap, multi-epoch ties, summary/selection priority, dedup, 16-cap, and missing trace without calling `measure_*` or `select_*`.

## Ordering

Depends on the bundle/manifest trace contracts. It unblocks service composition by producing the exact artifact request, marker list, focus times, effective policy, and composition warnings. On green verification this child advances directly to `done`.

## Implementation notes

- Execution capability: highest-capability cohesive inline ownership, as required by the caller; direct reads covered the bounded core/store/root surfaces without nested agents.
- Review weight: standard from the caller; not applicable at this checkpoint because it is a child story and advances directly to done after verification.
- Files changed:
  - `crates/krometrail-core/src/ports/timeline.rs` — added `TimelineRangeQuery`, `TimelineRangeSlice`, `MAX_TIMELINE_RANGE_ROWS`, and `TimelineStore::selected_range`.
  - `crates/krometrail-core/src/ports/mod.rs` — re-exported the new types and updated the `FakeTimeline` test impl.
  - `crates/krometrail-core/src/lib.rs` — re-exported `MAX_TIMELINE_RANGE_ROWS`, `TimelineRangeQuery`, `TimelineRangeSlice`.
  - `crates/krometrail-store/src/index/timeline.rs` — implemented `selected_range` for `SqliteIndex` with a kind `IN (...)` filter, exact `COUNT(*)`, bounded `LIMIT`, and `truncated` flag.
  - `crates/krometrail-store/src/recording.rs` — delegated `selected_range`, `InteractionAnchorSource`, and `InteractionRecordSource` through `RecordingStore` to `SqliteIndex` so root can project one store authority.
  - `src/debug_bundle/{mod.rs,policy.rs,markers.rs,focus.rs}` (new) — `TemporalDebugEvidenceStore` trait alias, v1 default generator requests, bounded privacy-safe marker assembly, and deterministic major-change focus extraction.
  - `crates/krometrail-store/tests/sqlite_timeline.rs` — added `selected_range` kind-filtering, count, truncation, and constructor-validation test.
- Tests added:
  - `policy.rs`: byte-stable default/orientation-omitted generators, exact v1 values, two-generator policy, version string.
  - `markers.rs`: mandatory caller/anchor inclusion, interaction/navigation/generic identity and privacy labels, caller-provided generic presentation, 256-cap truncation, 1024-row source truncation, equal-time class ordering, interval-anchor no-marker, no-leaked-secrets.
  - `focus.rs`: empty/unavailable/unchanged storyboards, changed storyboard focus, dedup, multi-epoch ties, 16-cap, orientation not read, trace-less manifest rejection, full-result-outcomes acceptance.
  - `mod.rs`: effective policy v1 values, focus-time count/ordering validation, trait alias static check.
  - `sqlite_timeline.rs`: kind-filtered count/truncation/ordering, absent-kind zero, constructor validation.
- Simplification: marker assembly is pure given loaded evidence; interaction anchor/record reads are metadata-only and hold no mutation gate; the `TemporalDebugEvidenceStore` trait alias introduces no facade methods; the kind `IN (...)` filter excludes browser events at SQL selection time without a new table or migration.
- Discrepancies from design: none. The `EffectiveBundlePolicy::new` constructor (Unit 1) validates focus-time count and ordering but not range containment; range containment is validated by `TemporalDebugBundle::new` (Unit 3). The `build_effective_policy` helper produces the effective value; the bundle constructor cross-checks it against the resolved range.
- Adjacent issues parked: none.

## Verification

- Rust 1.85: `cargo fmt --all -- --check` passed.
- Rust 1.85 changed crates (`krometrail-core`, `krometrail-store`, `krometrail`): all-target `clippy -- -D warnings` passed.
- Rust 1.85 changed crates: all-target `test` passed (101 core, 34+5 store, 56 root tests).
- Rust 1.85 workspace: `cargo check --workspace --all-targets --locked` passed.
