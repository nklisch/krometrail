---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-fixture-hashing-cross-platform
kind: story
stage: done
tags: [browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Make CDP fixture hashing cross-platform

## Symptom

GitHub run [29197934563](https://github.com/nklisch/krometrail/actions/runs/29197934563) completed the 60-second candidate gate and then failed with `Io: No such file or directory (os error 2)`. The raw evidence marked every gate as failed because fixture hashing invoked the unavailable Linux-only `sha256sum` command on macOS.

## Root cause

`crates/krometrail-cdp/src/spike/chrome_harness.rs::sha256_directory` shells out to `sha256sum`, which is not present on macOS. The fixture digest is evidence setup, so this external-command failure prevents the completed gate results from being recorded and causes the raw evidence failure state.

## Fix approach

Hash the ordered `index.html` and `animation.js` fixture bytes in-process with the SHA-256 dependency declared once in the workspace dependency source of truth. Preserve the existing `sha256sum-of-ordered-fixture-files:<index-hash>:<animation-hash>` representation so existing evidence/schema expectations remain valid.

## Regression test

Add a unit regression test beside `sha256_directory` that asserts the deterministic digest for the committed fixture, repeats the calculation, and verifies source-level absence of the external `sha256sum` command. The source-level assertion is the safe cross-platform reproduction because an absent-command environment cannot be simulated reliably without mutating process-global command lookup state.

## Acceptance criteria

- [x] Fixture hashing does not invoke an external hashing command and works on Linux and macOS.
- [x] The ordered-file digest representation remains unchanged.
- [x] Regression coverage proves deterministic, known fixture output and no external hashing command.
- [x] Default, spike, and cdpkit gates pass.
- [x] The macOS evidence story records this blocker and remains implementing until the workflow is rerun successfully on macOS.

## Implementation notes

- Execution capability: direct implementation; this is a focused one-file behavior fix plus workspace dependency wiring and a regression test, with no ownership or sequencing uncertainty.
- Review weight: standard, from the project default; the caller explicitly requested stop at review and no self-approval.
- Files changed: `Cargo.toml`, `Cargo.lock`, `crates/krometrail-cdp/Cargo.toml`, `crates/krometrail-cdp/src/spike/chrome_harness.rs`, and the macOS evidence story blocker.
- Test added: `spike::chrome_harness::tests::fixture_hashing_is_deterministic_and_does_not_require_external_hashing` checks the known ordered digest twice and source-level absence of `Command::new("sha256sum")`. The pre-fix implementation failed this test because it contained the Linux-only command invocation; process-global PATH mutation was intentionally not used.
- Representation: preserved `sha256sum-of-ordered-fixture-files:<index-hash>:<animation-hash>` exactly; no evidence/schema expectation update was needed.
- Verification: `cargo fmt --all -- --check`; default workspace check/test/clippy; `cdp-spike` check/test/clippy; and `cdp-spike-cdpkit` check/test/clippy all pass with `--locked`.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane bug review. The orchestrator independently reran nine cdpkit/spike test targets and candidate clippy and verified the deterministic known digest plus source-level removal of the external command. Root cause is fixed without changing the evidence representation. Verdict: Approve - story verified by implement; fast-lane advance.
