---
id: idea-flaky-discovery-precedence-test
created: 2026-07-19
updated: 2026-07-19
tags: [testing, browser]
---

Pre-existing flaky test (not caused by the 2026-07-19 shakedown work):
`launcher::discovery::tests::precedence_deduplicates_canonical_paths_and_classifies_versions`
in krometrail-cdp fails intermittently — roughly 40% of full `--lib` suite runs on base
commit 8ed2d7e9 — at `crates/krometrail-cdp/src/launcher/discovery.rs:368` with
`left: 1, right: 2` (one of two fixture installations dropped). It always passes solo and
in small filtered runs; failure only appears under parallel test load.

Ruled out so far: fixture-dir collision (dirs are unique per test via pid+counter in
`tempfile_root`, discovery.rs:425) and plain probe timeout (defaults are 2s ordinary /
10s cold, discovery.rs:44-45). Suspect `probe_version` (discovery.rs:249) spawn failure
or output handling under concurrent spawns classifying a candidate as `Rejected`.

Needs root-cause: determine whether this is test-harness fragility or a real discovery
robustness bug in `probe_version` under load, then fix accordingly.
