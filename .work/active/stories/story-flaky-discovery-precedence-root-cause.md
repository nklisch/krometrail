---
id: story-flaky-discovery-precedence-root-cause
kind: story
stage: implementing
tags: [testing, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Root-cause the flaky discovery test module

2026-07-19 update (post-scope evidence): the flakiness is broader than the
precedence test. Across five consecutive `cargo test -p krometrail-cdp --lib`
runs, three failed, each in a different `launcher::discovery::tests` test:
`precedence_deduplicates_canonical_paths_and_classifies_versions`,
`platform_defaults_use_cold_probe_budget_while_path_stays_short`, and
`failing_canonical_candidate_is_probed_once_at_highest_precedence` (the last
panicking at `discovery.rs:421` unwrap on `Os { code: 2, NotFound }` from the
fixture root helper). Treat the whole module's fixture/spawn behavior under
parallel load as the investigation scope, not one test.

## Brief

`launcher::discovery::tests::precedence_deduplicates_canonical_paths_and_classifies_versions`
(krometrail-cdp) fails intermittently — roughly 40% of full `--lib` suite runs on
base commit 8ed2d7e9 — at `crates/krometrail-cdp/src/launcher/discovery.rs:368`
with `left: 1, right: 2` (one of two fixture installations dropped). It always
passes solo and in small filtered runs; failure appears only under parallel test
load. Ruled out: fixture-dir collision (dirs unique per test via pid+counter in
`tempfile_root`, discovery.rs:425) and plain probe timeout (2 s ordinary / 10 s
cold defaults, discovery.rs:44-45). Suspect `probe_version` (discovery.rs:249)
spawn failure or output handling under concurrent spawns classifying a candidate
as `Rejected`.

Determine whether this is test-harness fragility or a real discovery robustness
bug in `probe_version` under load, then fix the actual cause: harden the product
path if the product mishandles spawn pressure, or fix the test fixture if the
test is at fault. Never game the assertion; if the fix is deferred mid-story,
document the failure inline per test-integrity rules.

Absorbed backlog: `idea-flaky-discovery-precedence-test`.

## Acceptance

- Root cause identified and stated in this story body with evidence.
- 20 consecutive full `cargo test -p krometrail-cdp --lib` runs pass (the
  observed failure rate makes 20 clean runs strong evidence).
- No assertion weakened; product fix or test-harness fix justified explicitly.

## Root cause

This was primarily test-harness fragility at the executable-fixture publication
boundary, compounded by a missing product hardening case. Under full parallel suite
load, `Command::spawn` intermittently returned Linux `ETXTBSY` (`ExecutableFileBusy`)
for a fixture such as `/tmp/krometrail-discovery-.../chrome`; the helper wrote the
shell script directly to its final executable path and then exposed that path to
discovery. The probe correctly returned `SpawnFailed`, but discovery had no bounded
retry for this transient OS result, so the precedence test observed one installation
instead of two, while the failing-candidate test's counter remained absent because its
script had never run. The unique PID/counter roots were not being deleted by siblings,
and serial or filtered runs did not reproduce the race.

Evidence: before the fix, host-side full parallel runs reproduced the original
`left: 1, right: 2`, the alternate platform-default failure, and the missing-counter
`NotFound`; temporary probe diagnostics identified `Os { code: 26,
ExecutableFileBusy }`. A serial full run and filtered discovery run passed. Replacing
the manual root with `tempfile::TempDir` removed unmanaged cleanup/reuse, but the
failure persisted until fixture files were staged, synced, permissioned, and atomically
renamed into their final executable paths.

## Completion notes

- Changed the discovery test harness: `tempfile::TempDir` now owns each fixture root,
  and `script_fixture` publishes a fully written executable via close-then-atomic
  rename.
- Hardened production `probe_version` to retry only transient `ExecutableFileBusy`
  spawn failures with a bounded 1/2/4/8 ms backoff; other spawn failures retain their
  existing explicit outcome.
- Added a deterministic regression test that holds a fixture executable busy briefly
  and verifies the probe recovers; no assertion was weakened, and no test retry or
  module serialization was added.
- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`,
  `cargo test --workspace --all-targets --locked`, and
  `cargo clippy --workspace --all-targets --locked -- -D warnings` all passed.
- Final evidence: 20 consecutive `cargo test -p krometrail-cdp --lib --locked` runs
  passed, each with 204/204 tests green.
