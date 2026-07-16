---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-ci-boundary-and-samples
kind: story
stage: done
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-run-manifest-and-schema]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Lock the CI-safe contract boundary and committed samples

## Checkpoint

Wire deterministic contract qualification without invoking Chrome, the network, a paid model,
or a second model family. Keep definitions and schemas in Git while keeping all run evidence
under the ignored `target/temporal-evaluation/` boundary. This story does not add an evaluation
CLI command or change the product command surface.

## Exact files and checks

Add focused contract tests in `crates/temporal-evaluation/tests/contracts.rs` and, where the
existing browser test package is the correct owner, a thin integration test at
`crates/krometrail-cdp/tests/temporal_benchmark_contract.rs`. Reuse the existing
`tests/fixtures/browser/README.md` boundary, `crates/krometrail-cdp/tests/support/chrome.rs`
identities/locks/wrappers, and the existing static-fixture serving pattern. Do not create a
second launcher, fixture server framework, product evaluation command, or framework-state
adapter.

Commit only:

- `docs/evidence/temporal-evaluation/v1/README.md` with ownership, schema/sample generation,
  exact matrix, privacy, and honest status policy;
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.json`;
- generated `benchmark-definition.schema.json`, `run-manifest.schema.json`, and canonical
  `sample-manifest.json`;
- the dependency-free target under `tests/fixtures/browser/temporal-benchmark/`; and
- Rust contract/test source and workspace metadata needed to validate those inputs.

A deterministic test must assert that all committed JSON is canonical, schema-valid, digestable,
correctly path-sanitized, and stable when loaded from a clean checkout. It must assert that the
fixture file list and ordered hashes match the definition. It must validate the matrix and prompt
registries without starting a browser. It must verify that the default test process has no model
or network invocation path.

Ignored output rules are explicit in the README and tests: per-run manifests, source frames,
generated artifacts, model answers/transcripts, patch workspaces, logs, and aggregate results go
only under `target/temporal-evaluation/` (which is covered by the existing `target/` ignore
rule). A test may write a temporary output directory and verify `git check-ignore`, but must not
modify `docs/public/llms-full.txt` or generated docs. `bun run docs:build` remains unrelated
VitePress documentation tooling and is not a benchmark qualification step.

## Honest state checks

Contract CI can pass only the contract claim. It must not write a live-capture `pass`, a platform
matrix result, an interpretation result, or a model result. Required Chrome/model/platform rows
are represented as `blocked`, `inconclusive`, or `unavailable` with reason and recovery action
when a later opt-in consumer creates them. Optional Linux Chromium alone may be `skipped` with a
preserved reason. Unsupported host platforms are explicit unsupported rows, not a fabricated
Linux/macOS result. A complete live row below threshold is `fail`, not `inconclusive`.

## Acceptance evidence

- [x] Locked ordinary Rust tests pass without Chrome, network access, paid agents, or model
      credentials and cover schema generation, canonical samples, definition hashes, registry
      completeness, and privacy/status invariants.
- [x] A clean-checkout test proves the committed schemas equal generated schemas and the sample
      manifest's bytes equal canonical serializer output.
- [x] The output-boundary test proves run outputs are ignored, no absolute/private path enters a
      manifest, and no generated documentation is edited.
- [x] The README names exactly what CI proves and does not broaden it into live capture,
      cross-platform, artifact, or model claims.
- [x] No standalone `[refactor]` item under review is modified as part of this checkpoint.

## Implementation notes

- Added the authoritative v1 README and a consolidated `contracts.rs` suite covering canonical
  definition/schema/sample identities, fixture hashes, registry completeness, deterministic matrix
  ordering, prompt/privacy/status invariants, the dependency-free crate boundary, and ignored
  `target/temporal-evaluation/` output.
- Generation was run into a clean ignored output directory and all four committed artifacts compared
  byte-for-byte. No CDP launcher or live-browser integration was added because the contract crate
  owns this CI-safe boundary and all ordinary tests remain browser-free.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked` (668 passed, 1 ignored)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- Clean generation/cmp checks for both definition and manifest artifacts

## Ordering

This is the final checkpoint. It depends on the manifest/schema story and makes the feature ready
for implementation verification; later feature work may consume the committed contract without
inventing aliases or migration paths.
