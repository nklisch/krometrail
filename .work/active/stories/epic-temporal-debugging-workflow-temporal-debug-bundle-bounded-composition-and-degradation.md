---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-bounded-composition-and-degradation
kind: story
stage: implementing
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus
release_binding: null
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
