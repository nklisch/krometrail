---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-feature-review-remediation
kind: story
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Close feature-level timing and runtime-retention findings

## Why

The feature-level cross-model review reproduced two material cross-unit gaps after child approval:

1. The documented acknowledgement metric begins when a frame returns from the event stream, but production starts its histogram after token extraction with a second clock sample. This under-reports the receive-to-ack-completion interval.
2. `stop_target` leaves terminal `StreamRuntime` entries in the coordinator map. Target churn therefore grows fixed ledgers/histograms for the session and exposes stale stopped statuses, contradicting the O(active streams) boundedness claim. The per-target ordinal registry needs matching terminal cleanup without resetting continuity on suspend or nonterminal detach/rebind.

A lower-risk transient duplicate-status window during generation replacement should be eliminated in the same cohesive registry repair.

## Scope

- Measure acknowledgement latency from the immediate post-`events.next()` observed-time sample through successful ack completion. Do not include frame wait or any downstream parse/handoff work, and do not take a second start sample after token extraction.
- After `stop_target` has captured its final status/outcome, remove only the exact stopped `(TargetId, attachment_generation)` runtime from the coordinator map. Never remove a newer replacement.
- Retain ordinal state through suspend and nonterminal `TargetDetached` replacement so ordinals remain continuous across generations. Remove per-target ordinal state on terminal `TargetClosed`/`TargetFailed`; clear it with full session teardown.
- Make `capture_statuses()` expose at most the highest attachment generation per target, sorted by `TargetId`, even during replacement races.
- Preserve all ack-first, counter/gap, deadline, visibility, reconnect, privacy, and real-Chrome behavior.

## Acceptance criteria

- [x] Deterministic clocks prove ack latency includes token-extraction interval from the receipt sample and excludes frame wait and post-ack work.
- [x] Repeated target create/start/stop churn keeps the stream registry/status cardinality bounded by live/current targets; stopped entries are absent.
- [x] Exact-key removal cannot erase a concurrently installed newer generation.
- [x] Status snapshots contain one highest-generation record per target and remain sorted.
- [x] Suspend/reconnect and nonterminal detach/rebind continue ordinals; terminal close/failure and whole-session teardown release ordinal registry state.
- [x] Full workspace, no-default, spike, clippy, and opt-in real-Chrome capture gates pass with zero process/profile references.

## Review weight

Standard: this repairs a timing metric and the feature's bounded-resource claim.

## Implementation notes (2026-07-13)

- Preserved terminal runtime removal in `crates/krometrail-cdp/src/capture/pipeline.rs`; the stop path still emits the final `CaptureStateChanged` before removing the exact `(TargetId, attachment_generation)` runtime entry.
- Updated the managed real-Chrome fidelity test (`crates/krometrail-cdp/tests/capture_real.rs`) to subscribe to session events before `stop`, then assert the buffered target-owned `CaptureStateChanged` reaches `Stopped` with truthful final statistics. Removed the stale `capture_statuses()` query after terminal removal.
- Added deterministic non-real coverage (`terminal_stop_publishes_final_status_before_runtime_removal` in `crates/krometrail-cdp/src/capture/tests.rs`) proving the final status event is observed before the runtime is removed from the registry.
- Verified the fix and the unchanged bounded-capture contract with: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked --no-default-features`, `cargo test -p krometrail-cdp --all-targets --locked --features cdp-spike`, and the opt-in real-Chrome gate `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real --features cdpkit-transport --locked` run twice. All passed with zero leaked Chrome processes or referenced profiles.

## Review finding (2026-07-13)

The implementation fixed both production findings, but independently running the required opt-in Chrome gate exposed a stale test contract: terminal runtimes are intentionally removed after publishing their final `CaptureStateChanged`, while the managed fidelity test still queried `capture_statuses()` after stop and expected the removed entry. Update the test to subscribe before stop and assert the buffered final `Stopped` status event (including truthful final statistics) instead of requiring stale registry retention. Keep the registry cleanup behavior; rerun the full opt-in suite and correct the falsely completed gate evidence before review resumes.

## Final review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Focused fresh review verified receipt-to-ack timing, exact terminal runtime removal, highest-generation status snapshots, ordinal retention/release semantics, and final `Stopped` event publication before removal. The exact opt-in Chrome command passed twice (5/5 each), workspace gates passed, and post-run process/profile checks were clean. All six criteria are satisfied.

**Status:** resolved by the implementation notes above; the story is staged for review.
