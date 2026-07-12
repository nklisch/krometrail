---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-process-tree-runtime-root
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

# Make Chrome cleanup and fixture resolution worktree-safe

## Origin

Final adversarial review found gate Chrome descendants/profiles surviving cancellation and compile-time `CARGO_MANIFEST_DIR` paths embedded from deleted worktrees into a shared target cache. A short Chrome test silently returned on attestation setup failure.

## Scope

Launch gate Chrome in an isolated process group/session and terminate/reap the entire group on normal exit, timeout, cancellation, and failure. Prove real-Chrome cancellation leaves zero descendants and removes profile. Resolve repository/fixture paths at runtime or pass an explicit root; no cached binary may depend on its build worktree. Add shared-target cross-worktree coverage. Setup/attestation failures must fail tests or emit a deliberate explicit skip reason, never return silently. Remove leaked gate-only processes/profiles after verifying ownership.

## Acceptance criteria

- [x] Chrome process group and profile are removed on every lifecycle path, including cancelled startup.
- [x] Shared-target binaries run correctly after build worktree removal using explicit runtime roots.
- [x] Real-Chrome tests cannot false-green on setup/attestation errors.
- [x] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence edit lands.

## Implementation notes

- Unix Chrome launches use the safe standard-library `process_group(0)` API. Drop, timeout, cancellation, startup failure, and normal teardown signal only the owned negative-PGID group, reap the direct child, force-kill lingering helpers, and remove a profile only after a live command-line ownership scan proves no process references it.
- Gate profiles are unique per launch. Startup removes only stale `/tmp/krometrail-cdp-gate-*` directories after the same ownership check and logs the removed/retained count. Added real-Chrome cancellation and active-reference cleanup regressions; missing Chrome emits an explicit `SKIP`, while invalid Chrome paths and attestation failures fail.
- Qualification paths now use a validated runtime/CLI `--repo-root`; attestation, fixtures, decisive validation, and decision loading all use that root. Added an `attest` command and a shared-target cross-worktree script/workflow check that deletes the build worktree before invoking the cached binary.
- Verification: `cargo fmt --all -- --check`; default, `cdp-spike`, and `cdp-spike-cdpkit` tests plus denied-warning clippy; the full candidate suite passed 32 tests including real Chrome; and `scripts/cdp-transport-gate-cross-worktree.sh` passed after deleting its build worktree. Before final verification, three pre-existing gate profiles were retained only while 24 matching Chrome command lines were verified, then terminated and removed; subsequent cleanup found none. No evidence artifacts, production adapter, or core files changed.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane runtime review reran 32 candidate tests including real Chrome, verified isolated process-group signaling and descendant/profile cleanup, confirmed zero remaining gate profiles/processes, validated explicit runtime repo roots and cross-worktree cached-binary coverage, and passed denied-warning clippy. Verdict: Approve - story verified by implement; fast-lane advance.
