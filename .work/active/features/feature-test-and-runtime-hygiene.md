---
id: feature-test-and-runtime-hygiene
kind: feature
stage: drafting
tags: [testing, cleanup, browser, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Test-suite and runtime hygiene

## Brief

Cluster of parked hygiene items. Each is small; together they are one pass over
test integrity and process/resource teardown. Two carry real evidence of active
harm and should not be treated as cosmetic.

**Active harm:**

- **`idea-eval-harness-browser-teardown`** — the evaluation harness leaks Chrome
  processes. Confirmed again during the seventh shakedown on 2026-07-20: Chrome
  processes from a **July 15** run were still alive in the process table five
  days later, holding profile directories under
  `target/temporal-evaluation/live/`. This is the second independent sighting.
  Real leaked OS resources, not test-harness cosmetics.

  **Investigation 2026-07-20 (do not re-derive this):** the ordinary teardown
  path is already correct — `ManagedChromeProcess` has a `Drop` impl that
  force-kills, including the owned process group
  (`crates/krometrail-cdp/src/launcher/process.rs:282-286`, `:38-50`,
  `:182`). `live_evaluation.rs:711` also calls `session.stop()` on the normal
  path, and no `?` early-returns between session creation and that call, so
  ordinary failures still reach it.

  The leak is **orphaning, not missing cleanup**: if the harness process is
  SIGKILLed (test timeout, CI cancel, manual kill), no `Drop` runs and Chrome
  survives indefinitely as an orphan. `QualificationLifecycle::cleanup` and its
  `Drop` (`src/app/live_evaluation.rs:244-265`) shut down the fixture server and
  remove the profile tree but never terminate the browser, so unwind alone does
  not save it either.

  Structural fix direction: make the browser die with its parent regardless of
  clean shutdown — on Linux `prctl(PR_SET_PDEATHSIG, SIGKILL)` in the child
  before exec, alongside the existing process-group kill. A guard that only runs
  on the happy path or on unwind cannot fix an orphan by construction. Also
  consider terminating the browser in `QualificationLifecycle::Drop` so panic
  unwind is covered even before the PDEATHSIG backstop applies.

  Not implemented in this pass only because `crates/krometrail-cdp/` was owned
  by a concurrent lane; the analysis above is complete enough to implement
  directly.
- **Flaky test (pre-existing, unresolved):**
  `krometrail-cdp` lib test
  `launcher::discovery::tests::precedence_deduplicates_canonical_paths_and_classifies_versions`
  fails intermittently (~40% of full-suite runs on 2026-07-19, base commit
  `8ed2d7e9`; always passes solo or in small filtered runs) at
  `crates/krometrail-cdp/src/launcher/discovery.rs:368` with `left:1 right:2` —
  one of two fixture installations is dropped only under parallel test load.
  Fixture dirs are unique per test (pid+counter) and probe timeouts are 2s/10s
  defaults, so neither collision nor plain timeout is the obvious cause; suspect
  `probe_version` spawn failure or output handling under concurrent spawns
  classifying a candidate as `Rejected`. **Root-cause before fixing** — determine
  whether this is harness fragility or a real discovery robustness bug, then fix
  accordingly. Per project test-integrity rules, do not silence it.

**Adjudicate, may close without code change:**

- **`gate-cruft-clarify-stream-runtime-send-sync-assertion`** — decide whether
  the uncalled `_assert_send_sync` compile-time guard in `capture/pipeline.rs`
  is retained with a clearer explanation, converted to an invoked/static
  assertion, or removed after proving existing spawned/`Arc` boundaries enforce
  both. The scanner called it dead code, but its typechecked body may protect a
  real guarantee. Do not remove as release cleanup without that adjudication.

**Straightforward cleanup:**

- **`gate-tests-remove-tautological-supervision-composition-assertions`** —
  `session_supervision.rs` constructs `ConnectionLost` and only matches the value
  it just constructed; `src/app.rs` compares an `Arc` pointer with itself rather
  than the service recipients. Remove or replace; the stronger reducer,
  session-capture, and dependency-identity tests already cover the real
  contracts.
- **`idea-clean-real-chrome-test-root-drop-order`** — explicitly
  `drop(launched)` before `drop(root)` in the affected real-Chrome tests and
  assert the root shell disappears.
- **`idea-mcp-cancellation-protocol-regression`** — add the cross-layer MCP
  cancellation regression: drive a real `notifications/cancelled` through the
  in-memory rmcp service while a fake `BrowserSessionPort::execute` is blocked,
  assert the token reaches `McpCancellation`/`BrowserOperationContext`, the
  operation returns caller-visible `cancelled`, and another session is
  unaffected.
- **`idea-capture-engine-hardening`** — exact/aggregated estimated counts when a
  gap ledger coalesces mixed count-bearing and non-count reasons; normalize
  `FrameRejected` estimated-count behavior between reader- and worker-side
  rejection; make the coordinator's active-stream cap robust to concurrent
  `start_target`.
- **`idea-fill-clear-dialog-race`** — reproduce the dialog race first, then
  either document the asymmetry deliberately or add a focused regression and
  bounded fix. Do not change dispatch posture without the reproduction.

## Simplification opportunity

Several of these are decisions rather than code. Closing an item with a recorded
rationale and no code change is a valid terminal outcome and is preferable to
manufacturing changes to make a backlog look drained. Where a fix requires a
reproduction first (`idea-fill-clear-dialog-race`, the flaky discovery test), the
reproduction is the deliverable that unlocks the decision.

## Acceptance

- No leaked browser processes or profile directories survive an evaluation run.
- The flaky discovery test is root-caused and either fixed or documented with an
  explicit reason linked to this item — never silenced.
- Tautological assertions removed; real coverage retained.
- Each adjudicated item reaches a terminal state with recorded rationale.
