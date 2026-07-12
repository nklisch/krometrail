---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-rss-evidence-validity
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Require valid cross-platform RSS evidence

## Symptom

GitHub run [29198272740](https://github.com/nklisch/krometrail/actions/runs/29198272740) succeeded, but its sanitized macOS evidence reported `rss_sample_count=0`, zero RSS medians and peak, and still marked `bounded-memory-proxy` as pass. The invalid downloaded evidence was not committed.

## Root cause

`process_rss()` only reads Linux `/proc/self/statm`; on macOS it returns `None`, the sampler silently drops every sample, and zero-valued aggregates make the memory threshold comparison pass. Evidence validation and the manual workflow contract did not require RSS windows, nonzero values, or a sustained sample cadence.

## Fix approach

Preserve `/proc/self/statm` on Linux and add a narrowly wrapped macOS `ps` sampler with explicit KiB-to-byte conversion. Make the sustained and bounded-memory gates fail when RSS samples, windows, medians, or peak are absent/zero, require at least 50 samples for the declared 60-second run after the fixed warmup, and reject equivalent passing evidence in `validate_evidence`. Extend the manual macOS workflow's exact-contract assertions without changing thresholds or the evidence schema.

## Regression test

`crates/krometrail-cdp/tests/transport_contract.rs::evidence_rejects_zero_rss_samples_and_window_values` must reject a passing memory gate with zero RSS samples and zero window/peak values.

## Blocker update

The macOS evidence story was blocked by run [29198272740](https://github.com/nklisch/krometrail/actions/runs/29198272740): the run succeeded while producing the zero-sample RSS flaw described above. No evidence was fabricated, committed, downloaded, pushed, or dispatched by this fix.

## Implementation notes

- Execution capability: host inline implementation; the fix is a focused, testable change in the CDP spike harness, evidence contract, and its manual workflow.
- Review weight: standard, from the project default; this story is explicitly left at `stage: review` for handoff.
- Files changed: `crates/krometrail-cdp/src/spike/chrome_harness.rs`, `crates/krometrail-cdp/src/spike/evidence.rs`, `crates/krometrail-cdp/src/spike/mod.rs`, `crates/krometrail-cdp/src/spike/scenarios.rs`, `crates/krometrail-cdp/tests/transport_contract.rs`, `.github/workflows/cdp-transport-gate.yml`, and the macOS evidence story blocker.
- Test added: `evidence_rejects_zero_rss_samples_and_window_values`, first run red before the validator fix and green afterward.
- Verification: default workspace tests, `cdp-spike` tests, `cdp-spike-cdpkit` tests, and all-features clippy with `-D warnings` pass.
- Adjacent issues parked: none; the invalid macOS evidence remains uncommitted and no waiver is introduced.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane bug review. The orchestrator independently reran ten cdpkit/spike test targets and clippy and verified Linux `/proc`, macOS `ps` KiB conversion, fail-closed sample/window validation, and hosted-workflow assertions. The zero-sample pass is no longer representable. Verdict: Approve - story verified by implement; fast-lane advance.
