---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-bounded-composition-and-degradation
kind: story
stage: done
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Compose One Bounded Degraded Bundle

## Checkpoint

Implement the single application service over existing temporal-query, timeline/interaction, artifact-generation, and temporal-context ports. Enforce the exact resolve → labels → artifacts/cache → manifest focus → one context query → compose order, one absolute deadline/cancellation, deterministic results, and explicit fatal versus usable-degraded semantics.

## Files

- `src/debug_bundle/{service.rs,header.rs,error.rs,tests.rs}` (new)
- `src/debug_bundle/{policy.rs,markers.rs,focus.rs}`
- `src/main.rs`
- `crates/krometrail-core/src/debug_bundle.rs` only for integration corrections

## Service

```rust
pub(crate) struct BundleWorkLimits {
    pub max_active_requests: NonZeroUsize, // 2
    pub max_wall_time: Duration,           // 20 seconds
}

pub(crate) struct TemporalDebugBundleService {
    queries: Arc<dyn TemporalQuery>,
    timeline: Arc<dyn TemporalDebugEvidenceStore>,
    artifacts: Arc<dyn ArtifactGeneration>,
    context: Arc<dyn TemporalContextQuery>,
    permits: Arc<Semaphore>,
    limits: BundleWorkLimits,
}
```

`TemporalDebugEvidenceStore` is a zero-method intersection of existing timeline and interaction record/anchor ports. Resolve exactly once; complete bounded label reads before visual work; call generation at most once with the global deadline; derive focus only from available storyboard manifests; call compact context exactly once with default limit 24 and the same range; compose existing results and a concise observation/proximity-only header.

## Failure contract

- Range failure, cancellation, deadline, source/session `NotFound`, or session deletion race fails the whole request and discards a partial bundle.
- Caller-authorized edge retention/gaps remain usable and explicit. Per-epoch artifact outcomes, non-lifetime artifact failure, marker-context failure/truncation, context failure/truncation, and browser-event unavailability can degrade when another component remains useful.
- If context is unavailable and no artifact outcome is available, return failure rather than an empty success.
- Existing cache hits and already-published artifacts remain valid; the bundle adds no cache/single flight and never retries generation or context.
- Header text uses only observed/measured/selected/co-occurred/nearest language and always states that measurements and proximity do not establish diagnosis or causality.

## Acceptance evidence

- Controlled spies prove one range resolution, at most one artifact generation, exactly one post-focus context query, and no duplicate frame/event/store/measurement/selection call.
- Artifact/context inputs and results retain the same exact `ResolvedRange`; source IDs appear only in range/manifests, with no image bytes/base64/path/URI/segment address.
- Single/multi-epoch available/unavailable outcomes preserve deterministic order, cache disposition, handles, manifests, and errors.
- Gaps, retention, capture warnings, collection loss, event unavailability, and truncation remain singular nested authorities rather than rewritten bundle summaries.
- Cancellation/deadline/eviction/deletion are fatal; independent marker/artifact/context failures produce only the specified usable degradations.
- Two active-request permits bound orchestration, and no timeline/store future or mutation guard spans artifact work.

## Ordering

Depends on exact default policy, marker assembly, and trace focus. It unblocks root composition. On green verification this child advances directly to `done`.

## Implementation notes

- Execution capability: highest-capability cohesive inline ownership, continuing the feature's one-owner baseline. Direct reads covered the existing artifact/context/progressive service patterns and the Unit 2 policy/marker/focus modules.
- Review weight: standard from the caller; not applicable at this checkpoint because it is a child story and advances directly to done after verification.
- Files changed:
  - `src/debug_bundle/{service.rs,header.rs,error.rs,tests.rs}` (new) — `TemporalDebugBundleService`, `BundleWorkLimits`, non-diagnostic header composition, deadline/cancellation wrapper, and focused spy tests.
  - `src/debug_bundle/mod.rs` — declared the new submodules and re-exported `BundleWorkLimits`/`TemporalDebugBundleService`/`controlled`/`compose_header`; moved the policy/trait-alias tests into `tests.rs`.
- Tests added (13 new spy-driven tests):
  - `one_resolution_one_generation_one_context_no_duplicate_calls` — proves exactly-once resolve, at-most-once generate, exactly-one post-focus context, and that store reads complete before generation.
  - `range_failure_is_whole_request_failure` — range NotFound aborts before generation or context.
  - `artifact_not_found_after_resolution_is_fatal` — evidence-lifetime NotFound discards partial work.
  - `non_fatal_artifact_failure_degrades_but_context_remains_useful` — PersistenceFailed artifact error produces `ArtifactRequestUnavailable` degradation with available context.
  - `context_unavailable_with_no_artifact_outcomes_fails` — both unavailable fails the whole request.
  - `marker_context_failure_degrades_but_bundle_succeeds` — timeline PersistenceFailed produces `MarkerContextUnavailable` degradation; caller/anchor markers survive.
  - `cancellation_before_resolution_is_fatal` — pre-cancelled signal fails before any port call.
  - `elapsed_deadline_is_fatal` — past deadline fails immediately.
  - `no_store_gate_spans_artifact_work` — barrier proves `selected_range_end` precedes `generate_start`; generation blocks while no store call is pending.
  - `bundle_result_contains_no_bytes_paths_or_uris` — serialized bundle has no base64/data-url/file-path/segment-address/MCP-URI.
  - `two_permits_bound_concurrent_orchestration` — two concurrent bundles each acquire and release their own permit.
  - `header::tests` (3) — approved-language check, no-focus statement, epoch-summary extraction.
  - `policy_tests` (3, moved from mod.rs) — effective policy v1 values, focus-time count/ordering, trait-alias static check.
- Simplification: `controlled` wraps every port await with `tokio::select!` on the future, the cancellation signal's `cancelled()` future, and `sleep_until(deadline)`; the artifact service additionally receives the same deadline/cancellation through `ArtifactGenerationContext`. `is_fatal_after_resolution` classifies NotFound/Cancelled/deadline-elapsed as fatal. Marker-context failure degrades to caller+anchor markers. The service adds no cache, single flight, or retry.
- Discrepancies from design: none. The `EffectiveBundlePolicy::new` constructor validates focus-time count and ordering; range containment is validated by `TemporalDebugBundle::new`. The service enforces "fail if both unavailable" before calling `TemporalDebugBundle::new`.
- Adjacent issues parked: none.

## Verification

- Rust 1.85: `cargo fmt --all -- --check` passed.
- Rust 1.85 workspace: `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Rust 1.85 workspace: `cargo test --workspace --all-targets --locked` passed (70 root, 101 core, 34 store, plus all other crate tests).
- Rust 1.85 workspace: `cargo check --workspace --all-targets --locked` passed.
