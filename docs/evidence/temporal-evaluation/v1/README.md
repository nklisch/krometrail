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
  are deterministic; and
- benchmark output has an ignored destination under `target/temporal-evaluation/`.

That is a **contract and reproducibility claim only**. CI does not claim that Chrome captured any
state, that a platform passed, that an artifact is useful, that a model understood an artifact,
or that temporal evidence improves debugging. It does not start Chrome, access a network, invoke a
model or paid agent, or change the product CLI.

The committed `sample-manifest.json` is a contract-only sample. It is not live browser evidence,
a capture result, an interpretation result, a scoring result, or a platform qualification.

## Authoritative inputs and generation

- `benchmark-definition.json` is the one current v1 benchmark-definition input.
- `benchmark-definition.schema.json` is generated from `BenchmarkDefinition`.
- `run-manifest.schema.json` is generated from `RunManifest`.
- `sample-manifest.json` is generated from `RunManifest::sample()` and canonicalized by the same
  serializer used by consumers.

Regenerate only the versioned artifacts with the Rust generators:

```text
cargo run -p temporal-evaluation --locked --bin generate-benchmark-definition -- \
  docs/evidence/temporal-evaluation/v1/benchmark-definition.json \
  docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json
cargo run -p temporal-evaluation --locked --bin generate-run-manifest -- \
  docs/evidence/temporal-evaluation/v1/sample-manifest.json \
  docs/evidence/temporal-evaluation/v1/run-manifest.schema.json
```

After generation, run the locked workspace checks. `bun run docs:build` is unrelated VitePress
tooling and is not a benchmark qualification step. Do not edit generated files by hand or update
`docs/public/llms-full.txt` for this contract.

## Canonical and privacy boundary

Canonical JSON recursively sorts object keys, preserves semantic array order, accepts only finite
numbers, normalizes `-0` to `0`, and uses lowercase `sha256:<64 hex>` identities. Source, observed,
and session times remain separate. The definition owns ground truth; no measurement, artifact, or
model answer can redefine it.

Committed fixture paths are repository-relative POSIX paths. Ignored evidence references are
opaque handles. Validation rejects absolute or private paths, traversal, backslashes,
URLs/endpoints and ports, credentials, control characters, page text, raw browser payloads, and
adapter error details. Raw model answers are not canonical manifest content: later consumers may
keep an ignored sidecar addressed by its digest and opaque reference.

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
contract checkpoint. Their manifests belong under the ignored output boundary and must retain the
same explicit status semantics.

## Ignored run output

Per-run manifests, source frames, generated artifacts, model answers/transcripts, patch
workspaces, logs, and aggregate results belong only under:

```text
target/temporal-evaluation/
```

The repository's existing `target/` ignore rule covers this boundary. Tests verify that a sample
output path is ignored and that no absolute path is serialized into a contract. Definitions,
schemas, fixture source, prompts, scoring vocabulary, and the canonical sample are the only
versioned evidence in this directory.
