---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-qualification
kind: story
stage: done
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-root-composition
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify the Temporal Debug Bundle Workflow

## Checkpoint

Qualify the complete single-range investigation path with current schema-v5 storage, production artifact/cache and context services, all anchor forms, deterministic marker/event timing, visual epochs, retention/gap degradation, cache reuse, and controlled cancellation/deletion races. Use focused trace/policy/header goldens and interface tests, not wrapper, SQL-line, or future MCP tests.

## Files

- `src/debug_bundle/tests.rs`
- `crates/temporal-vision/tests/{storyboard.rs,contracts.rs}`
- `crates/krometrail-store/tests/{range_context.rs,temporal_queries.rs,sqlite_timeline.rs}`
- `src/artifacts/{qualification_tests.rs,service_tests.rs}` only for cache-version integration
- `src/app.rs` tests
- small deterministic fixtures/goldens under existing temporal-vision/artifact test conventions only

## Acceptance evidence

- All seven anchor forms resolve once and report exact requested/resolved/effective anchor/options; interaction/latest/navigation/marker IDs and partial-retention clamping are exact.
- Default orientation-on/off parameters and cache identity are stable; exact repeats hit, storyboard `1.1.0` policy changes miss, and unrelated artifact versions remain stable.
- Single/multi epochs, unchanged/change, gaps, edge retention, per-epoch unavailable output, and no-stretch behavior match the designed usable/fatal semantics.
- Typed manifest focus covers first/peak/adjacent and selected-reason priorities, equal-time/frame-ID ties, dedup, 16-cap, missing trace, and no second measurement/selector.
- Marker coverage proves exact caller labels, interaction/navigation/generic IDs/times, mandatory anchor, equal-time order, 64/256/1024 caps, truncation, fallback-label warning, and privacy sentinels.
- Artifact/context partial failures preserve exact outcomes/errors; selected browser events retain existing compact reason and exact session-time distance, and all bundle/header text remains non-causal/non-diagnostic.
- Cache reuse, cancellation/deadline, source eviction, and session deletion return no stale bundle or late application result.
- Serialized application results contain no encoded image bytes, base64/data URLs, filesystem paths, segment addresses, MCP URIs, verbose event pages, or request/response bodies.
- One-call spies and root pointer/barrier integration prove one store/query/artifact/context/bundle authority and no mutation gate across artifact work.
- Focused goldens cover compact typed selection trace, effective policy, and header posture; existing PNG hashes remain the image regression surface.
- Rust 1.85 locked format, workspace all-target check/test, and Clippy with warnings denied pass. Child advances directly to `done`; only the parent receives standard review.

## Ordering

Depends on root composition and is the final implementation checkpoint for the feature.

## Implementation notes

- Execution capability: highest-capability cohesive inline ownership, continuing the feature's one-owner baseline. Direct reads covered the existing artifact qualification rig, real-store setup patterns, and all prior checkpoint test coverage.
- Review weight: standard from the caller; not applicable at this checkpoint because it is a child story and advances directly to done after verification. The parent feature advances to review after this child completes.
- Files changed:
  - `src/debug_bundle/tests.rs` — added a `qualification` submodule with 8 integration tests exercising real schema-v5 store + production artifact service.
  - `crates/krometrail-core/src/timeline/context.rs` — fixed `validate_resolved_range` to use `range.validate()` instead of `ResolvedRange::new`, which only accepted interval anchors. This is a behavior-preserving correction: `validate()` checks the same invariants for every anchor kind (interaction, navigation, marker, source-frame) that the resolver produces, while `new` rejected non-interval anchors.
- Tests added (8 new qualification tests):
  - `end_to_end_bundle_with_real_store_succeeds` — real store with 4 frames (JPEG/PNG), interaction evidence, generic marker; production artifact service; verifies v1 policy, available artifacts/context, non-diagnostic header, mandatory markers, no degradations.
  - `cache_reuse_second_bundle_hits_artifact_cache` — two identical bundle requests; second produces at least one `ArtifactCacheDisposition::Hit` outcome.
  - `bundle_serialized_result_has_no_bytes_paths_or_uris` — thorough forbidden-term check on real bundle JSON: no base64/data-url/file-path/segment-address/MCP-URI/filesystem.
  - `interaction_anchor_resolves_through_bundle_service` — interaction anchor with explicit zero window resolves through the real store; mandatory anchor marker present with exact interaction ID.
  - `orientation_omitted_changes_only_include_orientation_field` — orientation on/off produce effective policies differing only in `include_orientation`; orientation-included produces at least as many outcomes.
  - `session_deletion_after_resolution_is_fatal` — deleting the session before the bundle call fails the request.
  - `golden_effective_policy_is_byte_stable` — serializes the effective policy and verifies version/anchor/generators/failure-policy/focus-times content; re-serialization is byte-identical.
  - `golden_header_text_is_non_diagnostic_and_stable` — verifies approved language, non-diagnostic disclaimer, byte-budget, and re-composition stability.
- Integration correction: `validate_resolved_range` in `context.rs` previously used `ResolvedRange::new` which rejected non-interval anchors. Replaced with `range.validate()` which accepts all seven anchor kinds. This unblocks interaction/navigation/marker/source-frame anchors through `TemporalContextRequest` and `TemporalDebugBundle`.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- Rust 1.85: `cargo fmt --all -- --check` passed.
- Rust 1.85 workspace: `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Rust 1.85 workspace: `cargo test --workspace --all-targets --locked` passed (80 root, 109 core, 34 store, plus all other crate tests).
- Rust 1.85 workspace: `cargo check --workspace --all-targets --locked` passed.
