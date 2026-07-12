---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-decisive-evidence
kind: story
stage: implementing
tags: [browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-cdpkit-linux-qualification, epic-rust-cdp-capture-foundation-cdp-transport-gate-fixture-hashing-cross-platform, epic-rust-cdp-capture-foundation-cdp-transport-gate-rss-evidence-validity, epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-rss-compile-fix]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Capture decisive macOS transport evidence

## Scope

Run the unchanged shared harness and the currently qualifying candidate on current stable Chrome on macOS, commit sanitized schema-valid evidence, and prove the candidate decision is supported on both primary platforms. This story owns platform evidence, not production code.

## Exact files

- `docs/evidence/cdp-transport/v1/cdpkit-macos.json` when cdpkit remains the candidate, or the equivalently named selected-candidate macOS report after a late-bound fallback story
- `.github/workflows/cdp-transport-gate.yml` only if a manual, artifact-uploading macOS rerun workflow is needed for reproducibility; it must not become a default release/PR gate

## Requirements

- Run the same binary, fixture digest, schema version, scenario registry, thresholds, and candidate adapter used for the qualifying Linux evidence; do not create a macOS-only implementation path or weaken thresholds.
- Record macOS version/arch and Chrome/Rust/protocol/candidate versions, but no hostname, username, hardware serial, absolute binary/profile path, loopback endpoint, environment, or credentials.
- The report is decisive: every typed/raw/flat-session/drift/disconnect gate and the sustained 60-second/1,000-frame, capacity-1 saturation, ack-latency, and RSS-trend gate must have a measured pass/fail result.
- If Linux cdpkit evidence failed and a conditional fallback story was created, add that story to this story's `depends_on` only after running the mandatory cycle check, then run macOS against the fallback that passed Linux. Do not create speculative chromey/owned stories here.
- A macOS failure blocks the final decision rollup; it is evidence to revisit the candidate, not grounds for platform-specific exceptions.

## Acceptance criteria

- [ ] The macOS report validates against `docs/evidence/cdp-transport/v1/schema.json` and identifies exactly the tested revisions/configuration.
- [ ] The report demonstrates all decisive gates under unchanged thresholds and honestly records named-event-only/raw-envelope limitations.
- [ ] A clean checkout can reproduce the report from the documented command; committed output contains no machine-specific secrets or paths.
- [ ] No production adapter, core contract, capture pipeline, or platform-specific transport branch is introduced.

## Blocker

GitHub run [29197934563](https://github.com/nklisch/krometrail/actions/runs/29197934563) completed the 60-second cdpkit candidate gate on macOS but failed during fixture evidence setup with `Io: No such file or directory (os error 2)`. The raw evidence marked every gate failed because `crates/krometrail-cdp/src/spike/chrome_harness.rs::sha256_directory` invoked Linux-only `sha256sum`, which is absent on macOS. The cross-platform in-process hashing fix is tracked by `epic-rust-cdp-capture-foundation-cdp-transport-gate-fixture-hashing-cross-platform`; after it reaches review, an operator must push a ref containing the fix and manually rerun the unchanged cdpkit tests and full gate on a hosted macOS runner. No macOS evidence has been fabricated, committed, dispatched, or downloaded here. This story remains `stage: implementing` until that rerun produces validated evidence.

A later successful run, [29198272740](https://github.com/nathan/krometrail/actions/runs/29198272740), exposed a second blocker: sanitized macOS evidence had `rss_sample_count=0`, zero RSS medians and peak, and still marked `bounded-memory-proxy` as pass because the Linux-only `/proc/self/statm` sampler failed open. That evidence is invalid and was not committed. The RSS validity fix is tracked by `epic-rust-cdp-capture-foundation-cdp-transport-gate-rss-evidence-validity`; no waiver or platform-specific exception is permitted.

A subsequent compile run, [29198801356](https://github.com/nathan/krometrail/actions/runs/29198801356), failed before the gate because the macOS-only `process_rss` path used `.parse::<u64>()?` in an `Option<u64>` function. Rust therefore emitted E0277: the `Result` residual cannot be propagated as `Option`. The focused fix is tracked by `epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-rss-compile-fix`; it moves parsing and checked KiB-to-byte normalization into a target-neutral helper covered by Linux tests and a static sampler contract assertion. After that story is committed, an operator must manually rerun the unchanged workflow with the exact commit SHA (`workflow_dispatch` `ref` and `sha`); only a successful, schema-valid rerun may produce macOS evidence. No toolchain installation, dispatch, evidence fabrication, or evidence commit is performed here.
