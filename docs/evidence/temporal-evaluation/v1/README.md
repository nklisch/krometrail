# Temporal evaluation v1 contract

This directory is the authoritative, browser-agnostic contract for the temporal benchmark.
The Rust types in `crates/temporal-evaluation` validate the committed definition and manifest
shape; the generated schemas and canonical sample are checked into this directory.

## The narrow CI claim

A normal locked Rust test run proves only that:

- the v1 definition, schemas, and canonical sample are present, parseable, and byte-stable;
- fixture files are in the declared order with the declared SHA-256 identities;
- the condition, prompt, scoring, status, and deterministic matrix registries are complete;
- canonical serialization, digests, privacy rejection, retention references, and status invariants
  are deterministic;
- result records bind the manifest input digest to exact packages, scores, aggregates, threshold
  checks, per-frame availability proofs, retained evidence/source identities, artifact
  output/manifest hashes, algorithm/version identities, and cache identities without copying the
  manifest or storing raw/model/page prose, bytes, paths, or URLs;
- deterministic qualification replays fixed source records through the fake monotonic-clock and
  existing authority seams, checks the A–E budgets and one-interval identity, and keeps gaps,
  eviction, corruption, partial retrieval, unavailable inputs, and skipped rows non-passing; and
- benchmark output has an ignored destination under `target/temporal-evaluation/`.

That is a **contract and reproducibility claim only**. CI does not claim that Chrome captured any
state, that a platform passed, that an artifact is useful, that a model understood an artifact,
or that temporal evidence improves debugging. It does not start Chrome, access a network, invoke a
model or paid agent, or change the product CLI.

The committed `sample-manifest.json` is a contract-only input sample. The committed
`sample-evaluation-result.json` is a deterministic-CI result sample: it demonstrates canonical
result and traceability semantics, is always `thesis_eligibility=not_eligible`, and is not live
browser evidence, a capture result, a model-comprehension result, a debugging qualification, or a
product-thesis assessment.

## Test-only live qualification opt-in

The live qualification harness is not a product command. It is an ignored, test-only path that
requires both explicit authorizations:

```text
KROMETRAIL_REAL_CHROME_TESTS=1 \\
KROMETRAIL_LIVE_CAPTURE_EVALUATION=1 \\
  cargo test --features qualification-support -- --ignored
```

Do not set either variable for ordinary verification. The authorized path uses the production
browser, capture, store, retention, query, and artifact authorities and writes one canonical
`run-manifest.json` below:

```text
target/temporal-evaluation/live/<browser-product>/<run-id>/
```

A missing required browser is `blocked`; missing optional Linux Chromium is `skipped`; incomplete
source, gaps, unavailable resources, retention loss, or unsupported measurements remain
`inconclusive` or `blocked`; only complete threshold evidence can be `pass`. Finalization writes
only after capture flush, browser/profile/server cleanup, lock release, and staging cleanup have
been observed. A finalization error is a safe error report, never a passing result.

The manifest records `evidence_mode` as either `code_harness` or
`operator_authorized_live_capture`; a code/harness run is never live browser evidence. A local
`pass` qualifies only the declared browser, operating system, viewport, device scale, capture
settings, fixture revision, and reference-host configuration. It does not claim a model
call or model effectiveness, remote or paid-service behavior, high-DPI behavior, cross-platform
parity, temporal-advantage uplift, or production-scale stability. macOS and high-DPI belong to
separate platform evidence; an unresolved gap there does not block later agent-effectiveness
trials on a declared reference host. Live qualification does not invoke a model and emits no
model answer, transcript, or thesis result.

## Platform lane contract

Platform evidence is a test-only adapter over the same `run_live_qualification` composition; it
is not a second browser, recording store, artifact service, manifest authority, or product command.
The one registry in `crates/temporal-evaluation/src/platform.rs` owns this canonical order:

1. `linux_stable_chrome_reference_host` — required stable Chrome reference host;
2. `macos_chrome_default_dpi` — required stable Chrome at observed scale one;
3. `macos_chrome_high_dpi` — required stable Chrome at observed scale at least 1.5;
4. `linux_chromium_optional` — optional Chromium at observed scale one.

A platform run carries its typed lane/profile declaration in the existing `RunManifest`; the
existing qualification measurements remain authoritative for browser identity, environment,
viewport, observed scale, timings, gaps, retention, and cleanup. Requested wrapper flags are
configuration only. In particular, a high-DPI request with observed scale one is rejected and
cannot produce a passing high-DPI record. Unknown, duplicate, wrong-platform, wrong-product,
wrong-profile, wrong-viewport, and out-of-band scale declarations fail validation. Missing optional
Chromium remains an explicit skip and cannot satisfy Chrome or cross-platform coverage.

The shared runner checks both explicit opt-in gates before fixture validation, discovery, loopback
server startup, profile creation, store opening, or output preparation. Ordinary locked checks do
not launch Chrome; each authorized run remains under the existing real-browser lock and writes
only its ignored run output below `target/temporal-evaluation/live/`.

## Authoritative inputs and generation

- `benchmark-definition.json` is the one current v1 benchmark-definition input.
- `benchmark-definition.schema.json` is generated from `BenchmarkDefinition`.
- `run-manifest.schema.json` is generated from `RunManifest`, including the optional typed platform
  lane declaration used only by platform-evidence runs.
- `sample-manifest.json` is generated from `RunManifest::sample()` and canonicalized by the same
  serializer used by consumers.
- `evaluation-result.schema.json` is generated from `EvaluationResultRecord`.
- `sample-evaluation-result.json` is generated from the deterministic result constructor and
  canonicalized by the same serializer used by consumers.

Regenerate only the versioned artifacts with the Rust generators:

```text
cargo run -p temporal-evaluation --locked --bin generate-benchmark-definition -- \
  docs/evidence/temporal-evaluation/v1/benchmark-definition.json \
  docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json
cargo run -p temporal-evaluation --locked --bin generate-run-manifest -- \
  docs/evidence/temporal-evaluation/v1/sample-manifest.json \
  docs/evidence/temporal-evaluation/v1/run-manifest.schema.json
cargo run -p temporal-evaluation --locked --bin generate-evaluation-result -- \
  docs/evidence/temporal-evaluation/v1/sample-evaluation-result.json \
  docs/evidence/temporal-evaluation/v1/evaluation-result.schema.json
```

After generation, run the locked workspace checks. `bun run docs:build` is unrelated VitePress
tooling and is not a benchmark qualification step. Do not edit generated files by hand or update
`docs/public/llms-full.txt` for this contract.

## Canonical and privacy boundary

Canonical JSON recursively sorts object keys, preserves semantic array order, accepts only finite
numbers, normalizes `-0` to `0`, and uses lowercase `sha256:<64 hex>` identities. Source, observed,
and session times remain separate. The definition owns ground truth; no measurement, artifact, or
model answer can redefine it.

### Affected-region coordinate contract

Each case's `affected_region` is the exact half-open axis-aligned localization ROI in the fixed
800x450 captured-viewport-pixel space. `(x, y)` is measured from the screenshot's top-left;
width and height cover `x..x+width` and `y..y+height`, at the canonical device scale of one.
The region is the union of the affected fixture subject's visible extents over all declared
phases, clipped by the visible stage. A moving panel
therefore includes its complete path, a layout shift includes both the inserted notice and the
pre-/post-shift content extent, and a DOM-opaque canvas uses its complete rendered canvas box.
The values are not `.stage`-relative offsets, `getBoundingClientRect()` values from another
viewport, or canvas-local JavaScript drawing coordinates. Model answers use this same coordinate
space. The contract tests derive every canonical rectangle from the committed fixture CSS, HTML,
and animation geometry without starting Chrome.

Committed fixture paths are repository-relative POSIX paths. Ignored evidence references are
opaque handles. Validation rejects absolute or private paths, traversal, backslashes,
URLs/endpoints and ports, credentials, control characters, page text, raw browser payloads, and
adapter error details. Raw model answers are not canonical manifest content: later consumers may
keep an ignored sidecar addressed by its digest and opaque reference.

## Per-frame availability proof

`ConditionPackage.source_frame_availability` and the corresponding result trial field preserve
one ordered availability record for every `source_frame_ids` entry. The package validator binds
that projection to the source interval digest and rejects reordered, missing, or mutated records.
Result trace construction uses those exact records rather than deriving source-frame status from
aggregate `RetentionState`: retained citations remain `retained` in a `partially_retained`
interval, while `corrupt`, `evicted`, `gap`, and `not_collected` frames remain their original
availability. Claims still require a retained trace; an unavailable citation is rejected.

## Future execution states

Availability and evaluation status are separate facts. A required browser, platform, source, or
model that is not available must never become a passing row:

- `blocked` means a required precondition, unsupported platform, browser, or model authorization
  prevents the run; the manifest records the reason and a recovery action.
- `inconclusive` means some evidence exists but gaps, eviction, corruption, missing source,
  insufficient repetitions, or another unresolved limitation prevents a decisive claim.
- `unavailable` is an explicit dependency/evidence availability state and is represented as such;
  an aggregate consumer reports the resulting required row as `blocked` or `inconclusive`.
- `fail` is reserved for complete, decisive evidence below the declared threshold (or a complete
  validation failure), not for missing evidence.
- `skipped` is allowed only for the named optional Linux Chromium configuration and preserves why
  it was unavailable. It is not a pass and cannot stand in for a required platform.
- `pass` requires every required identity, retained evidence, minimum sample, gap policy, and
  threshold condition for that future consumer.

Those live, cross-platform, artifact, and model consumers are opt-in work outside this CI-safe
contract checkpoint. Their manifests and result records belong under the ignored output boundary
and must retain the same explicit status semantics. A deterministic-CI result can never be
thesis-eligible, even when synthetic arithmetic thresholds pass; its fixed non-claims explicitly
cover Chrome capture, network access, paid/model calls, model comprehension, product-thesis
improvement, causal diagnosis, deterministic replay, cross-model generalization, unobserved
frames, and treating artifacts as ground truth. The qualification suite's synthetic answers and
fake source/store records therefore prove contract and traceability behavior only; they do not
qualify browser capture, artifact usefulness, platforms, model interpretation, debugging, or the
product thesis.

### Aggregate artifact cache state

Live latency samples retain each authority-returned `ArtifactCacheDisposition` on every artifact
identity. Their aggregate `CacheDisposition` is computed from those exact identities: `cold`
means every artifact was generated (including regeneration after invalidation), `warm` means every
artifact was a cache hit, and `mixed` means the set contains both generation and hits. An empty or
unavailable artifact set remains `unavailable`. Invocation order is not a cache-state signal: the
first temporal-bundle call may be `mixed` when a direct artifact request has already warmed a
shared key, while the decisive checks remain uncached direct artifact generation below 5 seconds
and an all-hit warm bundle below 1 second.

## Ignored run output

Per-run manifests, result records, source frames, generated artifacts, model answers/transcripts,
patch workspaces, logs, and aggregate results belong only under:

```text
target/temporal-evaluation/
```

The repository's existing `target/` ignore rule covers this boundary. Tests verify that a sample
output path is ignored and that no absolute path is serialized into a contract. Definitions,
schemas, fixture source, prompts, scoring vocabulary, and the canonical sample are the only
versioned evidence in this directory.
