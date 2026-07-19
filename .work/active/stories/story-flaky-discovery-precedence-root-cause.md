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

# Root-cause the flaky discovery precedence test

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
