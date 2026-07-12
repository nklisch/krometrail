---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-process-tree-runtime-root
kind: story
stage: implementing
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

- [ ] Chrome process group and profile are removed on every lifecycle path, including cancelled startup.
- [ ] Shared-target binaries run correctly after build worktree removal using explicit runtime roots.
- [ ] Real-Chrome tests cannot false-green on setup/attestation errors.
- [ ] Default/spike/candidate tests and denied-warning clippy pass; no production/core change or evidence edit lands.
