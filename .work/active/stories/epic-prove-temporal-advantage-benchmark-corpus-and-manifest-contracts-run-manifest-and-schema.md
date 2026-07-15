---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-run-manifest-and-schema
kind: story
stage: done
tags: [testing, visual, browser, storage]
parent: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-vocabulary-and-prompts]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Define and validate the reproducibility manifest

## Checkpoint

Add a browser-agnostic `temporal-evaluation` workspace crate. It owns benchmark contract types,
canonical serialization, schema generation, source identity hashing, and privacy/status
validation. It must not be imported by the Krometrail product runtime, CDP transport, browser
fixture, or model client. Later evaluation features consume this one contract instead of making
parallel manifest types.

## Exact files and public surface

Add `crates/temporal-evaluation/Cargo.toml` to the workspace and these modules:

- `crates/temporal-evaluation/src/lib.rs` — public exports and contract constants;
- `crates/temporal-evaluation/src/corpus.rs` — the family/case/phase/condition/prompt/scoring
  registries and definition validation;
- `crates/temporal-evaluation/src/manifest.rs` — run manifest and row identities;
- `crates/temporal-evaluation/src/canonical.rs` — canonical JSON and digest functions;
- `crates/temporal-evaluation/src/privacy.rs` — path, endpoint, string, and sensitive-data
  rejection.

The crate uses `serde`, `schemars`, `serde_json`, and `sha2` already present in workspace
metadata. Public signatures are:

```rust
pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const BENCHMARK_ID: &str = "temporal-advantage-corpus-v1";

pub fn canonical_json<T: serde::Serialize>(value: &T)
    -> Result<Vec<u8>, ContractError>;
pub fn sha256_prefixed(bytes: &[u8]) -> String;
pub fn benchmark_definition_schema() -> schemars::Schema;
pub fn run_manifest_schema() -> schemars::Schema;

impl BenchmarkDefinition {
    pub fn canonical() -> Self;
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn definition_digest(&self) -> Result<String, ContractError>;
}

impl RunManifest {
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn sanitize(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn input_digest(&self) -> Result<String, ContractError>;
}
```

The types derive `Serialize`, `Deserialize`, `JsonSchema`, and `deny_unknown_fields`; registry
enums derive their wire names and `ALL` values from one declaration. The definition is loaded
from the committed JSON and checked against `BenchmarkDefinition::canonical()` or an equivalent
canonical registry projection, so a consumer cannot silently accept an undeclared variant.

## Manifest schema

`RunManifest` is one current v1 shape with no legacy aliases, migration forms, or compatibility
fields. Required top-level fields are:

```text
schema_version, kind, benchmark_id,
benchmark_definition { git_revision, sha256 },
harness { git_revision, sha256 },
scorer { git_revision, version },
fixture { root_relative_path, ordered_files[{path, sha256}], definition_sha256 },
run { seed, order_policy, ordered_trials[], duration_ms[], repetitions, condition_id,
      threshold_profile, viewport, device_scale_factor, image_format, image_quality,
      retention_budget_bytes },
environment { platform, architecture, os_release_class },
browser { availability, product, product_version, protocol_version, revision, capability_id },
krometrail { git_revision, cargo_lock_sha256, rust_toolchain, capture_config,
             adapter_versions },
model { availability, provider, model_id, model_version_or_dated_alias,
        invocation_date, authorization_ref, tools, input_limits },
prompt { prompt_set_id, prompt_version, system_prompt, task_prompt, sha256 },
artifact { condition_id, algorithm_versions, parameters, source_interval_id,
           output_ids[{id, sha256, availability}], source_frame_ids[], gap_ids[] },
scoring { rubric_version, dimension_ids[], aggregate_method, rationale_policy },
rows[], status, non_claims[], failure
```

`browser`, `model`, and output/source availability use explicit tagged states. A missing browser
is not an observed browser with empty version fields. A required unavailable environment is
`blocked` or `inconclusive`; only the named optional Linux Chromium row can be `skipped`. A
manual model row without operator authorization is `blocked`, and a row missing minimum samples,
retained source, decisive model metadata, or gap-free evidence is `inconclusive`. `pass` requires
all required inputs and thresholds; complete below-threshold evidence is `fail`.

Each row carries `trial_id`, `case_id`, `family`, `duration_ms`, `repetition`, `condition_id`,
`capture_ordinal_range`, `source_time_range`, `observed_time_range`, `session_time_range`,
`gap_ids`, `retention_state`, `artifact_ids`, `accepted_claims[{claim_id, evidence_ids}]`,
`answer_digest`, `raw_answer_ref`, `score`, `scoring_rationale`, and row `status`. The raw model
answer is preserved as an ignored sidecar addressed by its digest; the manifest retains only the
bounded structured answer/digest and opaque reference so page bodies or sensitive browser data
cannot enter committed or canonical metadata. The exact prompt text remains in the manifest for
model runs and is itself validated against the committed prompt digest.

## Canonical and privacy rules

Canonical bytes are UTF-8 JSON with recursively lexicographic object keys, semantic arrays in
explicit order, finite numbers only, `-0` normalized to `0`, lowercase `sha256:<64 hex>` hashes,
no insignificant whitespace ambiguity, and bounded string/array sizes. Millisecond durations,
counts, ordinals, and byte sizes are integers. Repeated canonical serialization and digest
calculation must be byte-identical across OSes.

Sanitization accepts only repository-relative POSIX paths for committed inputs, and opaque
relative output handles for ignored evidence. It rejects absolute paths, `..`, backslashes,
URLs/endpoints, ports, usernames, home/profile/temp roots, credentials, control characters,
page bodies, raw browser event data, and adapter error text. Fixture URLs are represented by the
case ID and relative route, never by a host or port. The manifest hash covers exact benchmark
inputs and relevant source identities; it does not imply that source frames or artifacts still
exist.

## Acceptance evidence

- [x] The crate builds without a dependency on Krometrail runtime crates or a browser/model
      client and its generated schemas contain no hand-maintained duplicate type surface.
- [x] `benchmark-definition.schema.json` and `run-manifest.schema.json` under
      `docs/evidence/temporal-evaluation/v1/` exactly match schemas generated from the Rust
      types; `sample-manifest.json` round-trips byte-for-byte through canonical serialization.
- [x] Validation rejects unknown fields, duplicate IDs, unsorted semantic arrays, non-finite
      numbers, contradictory counts/hashes, missing required identity for an observed browser or
      model, and a pass with a blocked/skipped/inconclusive required row.
- [x] Privacy tests reject absolute/private paths, endpoint forms, credentials, raw page text,
      and raw adapter errors while accepting the permitted relative fixture paths and hashes.
- [x] Tests prove source/artifact availability, gaps, retention, and model authorization are
      represented independently and never promoted to a passing claim by omission.

## Implementation notes

- Extended `temporal-evaluation` with strict tagged browser/model/evidence availability states,
  bounded run rows, explicit status/failure semantics, and independent source/artifact retention
  identities.
- Added canonical manifest and input digests, `-0` normalization, privacy rejection, and a
  generated `generate-run-manifest` tool. The committed sample is intentionally contract-only and
  makes no browser, model, or evidence claim.
- Generated and committed `run-manifest.schema.json` and `sample-manifest.json`.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (663 passed, 1 ignored)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Ordering

This story depends on the exact case/condition/prompt/score registries. Its schemas become the
only boundary consumed by later scoring, live capture, platform, and manual-agent features.
