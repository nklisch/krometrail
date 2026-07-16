---
id: perf-temporal-overlap-frame-reuse-opt-2-scheduler-service-integration
kind: story
stage: done
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: [perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight]
release_binding: 1.0.0
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Integrate shared work with artifact scheduling and lifecycle fences

## Scope

Integrate the shared frame-work flights into `TemporalVisionArtifactService`
without changing generated artifact semantics. The service must reuse exact
source-frame decode and normalization work across concurrent adjacent windows,
while the scheduler charges unique shared bytes once and keeps request-local
output/publication memory bounded.

## Integration requirements

- Pass one batch lease through `generate_inner` and `run_flight`; assemble each
  request's source and normalized sequences in source order from shared immutable
  frames.
- Split memory accounting into unique shared intermediate bytes and
  request-local output/temporary bytes. Enforce a byte-weighted reuse admission
  cap with a capture reserve; recompute when an entry cannot be retained rather
  than exceeding the cap.
- Keep the current one/two request permit and one generator permit behavior
  observable. Do not introduce nested unbounded worker pools or parallel decode.
- Revalidate exact source frames before selecting any intermediate hit. Final
  artifact lookup, source payload validation, artifact cache locking, and
  publication remain authoritative.
- Ensure deletion and invalidation cannot return stale pixels: source reads must
  reject deleted/evicted evidence, active work must observe cancellation, and
  publication must recheck the deletion fence. Drop all session entries after
  the final batch lease.
- Test source corruption, visual-epoch changes, mask/normalization changes,
  segment eviction, session deletion during decode/normalize/publication,
  cancellation of leaders and waiters, and concurrent publication races.

## Verification

- Current artifact service tests remain green, including deterministic
  manifests and existing final-artifact cache behavior.
- Two concurrent adjacent requests report one decode and normalization execution
  per shared frame key and no durable intermediate rows/files.
- One-permit and over-budget cells remain bounded and explicit rather than
  silently claiming reuse.

## Implementation notes

- Execution capability: inline feature-owner implementation; scheduler, service, temporal-vision assembly, lifecycle fences, and their tests form one cohesive ownership boundary.
- Review weight: standard default; this child story advances directly to done after green verification and does not enter review.
- Files changed: `src/artifacts/service.rs`, `src/artifacts/scheduler.rs`, `src/artifacts/single_flight.rs`, `src/artifacts/cache.rs`, `src/artifacts/decode.rs`, `src/artifacts/epoch.rs`, `src/artifacts/generators.rs`, `src/artifacts/service_tests.rs`, `src/artifacts/overlap_perf.rs`, `crates/temporal-vision/src/normalize.rs`, and `crates/temporal-vision/src/lib.rs`.
- Tests added/updated: service-level adjacent 119-frame overlap with deterministic leader gating, one decode and normalization leader per shared key, final-artifact-only fake persistence, and post-batch shared-byte/entry release; existing cache, corruption, deletion, cancellation, publication, epoch, mask, and eviction suites remain green.
- Accounting/lifecycle: decoded and normalized immutable pixels now reserve shared permits once through the scheduler memory semaphore; request-local output permits remain per flight; production-sized limits preserve a 128 MiB capture reserve and cap shared work at 1.5 GiB; non-admitted entries are usable only by current waiters and acquire request-local fallback permits, then are removed for recomputation. Ready flights remain joinable until the final batch lease drops them.
- Simplification: removed the old per-flight decoded/normalized reservation from the service path and reused the existing blocking/generator permit surfaces; no nested pool, durable intermediate row/file, TTL cache, or pair-analysis context was added.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification evidence

- Rust 1.85.0 `cargo fmt --all -- --check` passed.
- Rust 1.85.0 `cargo check --workspace --all-targets --locked` passed.
- Rust 1.85.0 `cargo test --workspace --all-targets --locked` passed.
- Rust 1.85.0 `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Focused service tests passed, including concurrent adjacent reuse and no intermediate durable persistence.
- Final five-repetition acceptance qualification and parent-feature advancement were intentionally not run per request; no Chrome, models, network, pair context, other feature, or push was used.
