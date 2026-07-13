---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-feature-review-remediation
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity]
release_binding: null
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

- [ ] Deterministic clocks prove ack latency includes token-extraction interval from the receipt sample and excludes frame wait and post-ack work.
- [ ] Repeated target create/start/stop churn keeps the stream registry/status cardinality bounded by live/current targets; stopped entries are absent.
- [ ] Exact-key removal cannot erase a concurrently installed newer generation.
- [ ] Status snapshots contain one highest-generation record per target and remain sorted.
- [ ] Suspend/reconnect and nonterminal detach/rebind continue ordinals; terminal close/failure and whole-session teardown release ordinal registry state.
- [ ] Full workspace, no-default, spike, clippy, and opt-in real-Chrome capture gates pass with zero process/profile references.

## Review weight

Standard: this repairs a timing metric and the feature's bounded-resource claim.
