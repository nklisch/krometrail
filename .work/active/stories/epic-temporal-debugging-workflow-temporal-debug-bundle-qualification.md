---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-qualification
kind: story
stage: implementing
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
