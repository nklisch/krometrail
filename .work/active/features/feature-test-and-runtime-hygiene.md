---
id: feature-test-and-runtime-hygiene
kind: feature
stage: review
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

Cluster of parked hygiene items: one pass over test integrity and process/resource
teardown. Two carried evidence of active harm; the rest were adjudications, three
of which turned out to be real defects rather than cosmetics.

## Architectural choice

**Browser teardown is a kernel guarantee, not a cleanup path.** Every existing
teardown route — `terminate`, `force_kill_now`, `Drop`, the process-group kill —
requires the launcher to still be executing its own code. A SIGKILLed or crashed
launcher runs none of them, which is precisely how Chrome processes from a July 15
run were still alive on July 20. No amount of additional cleanup code can fix an
orphan by construction, so ownership moves to `prctl(PR_SET_PDEATHSIG, SIGKILL)`
installed in the child between fork and exec. The process-group kill remains as the
cooperative path; PDEATHSIG is the backstop that needs no cooperation.

## Design decisions

### Flaky discovery test — already fixed upstream; item body was stale (no code change)

The item described this as unresolved at base `8ed2d7e9`. It was in fact root-caused
and fixed in `2428d40a`, which is an ancestor of HEAD. The cause was **ETXTBSY under
concurrent fork**, a real robustness bug and not harness fragility: when one test
writes a fixture executable, a concurrent `Command::spawn` elsewhere in the process
forks and inherits that write file descriptor; if the writer closes and execs inside
that window, the exec fails with `ExecutableFileBusy`. `probe_version` classified
that as a candidate loss, dropping one of the two installations.

The fix lives in production code (`spawn_version_probe` retries `ExecutableFileBusy`
with backoff), with test fixtures additionally hardened to write-then-rename.

Verified at HEAD, not assumed: 40 filtered runs at `--test-threads=32` and 25 full
`krometrail-cdp` lib-suite runs at `--test-threads=64`, all under 8–12 concurrent CPU
hogs. Zero failures. Left as-is.

Residual, deliberately not changed: the retry budget is 4 attempts totalling ~15 ms.
That is ample for the fork/exec window but would not cover a package manager rewriting
`/usr/bin/google-chrome` for longer. Recorded under Risks rather than speculatively widened.

### `_assert_send_sync` in `capture/pipeline.rs` — removed, with proof

Removed only after proving the guarantee is enforced by real code. `Arc<StreamRuntime>`
is moved into four separate `tokio::spawn` calls (frame, visibility, geometry, worker
readers), and `tokio::spawn` requires a `Send` future; a future capturing
`Arc<StreamRuntime>` is `Send` only if `StreamRuntime: Send + Sync`. It is additionally
held in `Mutex<HashMap<StreamKey, Arc<StreamRuntime>>>` shared across those tasks. Five
independent sites enforce both bounds.

Its comment was also actively wrong — it described transport/sink future ordering, which
has nothing to do with `Send + Sync`, making it worse than no guard.

### `idea-fill-clear-dialog-race` — premise is stale; asymmetry documented (regression added)

The item describes a `Ctrl+A` + `Delete` chord. That is not the current implementation.
`clear_editable` uses a programmatic `Runtime.callFunctionOn` selection, then a separate
`Backspace`, then **re-reads the field length and fails explicitly** if anything remains.

So the harmful outcome — a dialog swallowing the deletion and leaving only select-all
dispatched — cannot occur silently. The sequence is genuinely non-atomic and deliberately
so (pointer dispatch cannot be made atomic either), but it is made safe by verifying the
result instead of trusting the dispatch. Dispatch posture unchanged, as instructed.

That verification path had **no test coverage**, so the deliberate asymmetry is now pinned
by a regression rather than left as an undocumented accident.

### `idea-capture-engine-hardening` — all three were real defects

1. **Active-stream cap TOCTOU.** Admission read `streams`, released the lock, then did
   subscription and task setup across several awaits before inserting. Every concurrent
   start measured the same pre-insertion registry, so the cap was overshot by however many
   raced. Demonstrated: 6 of 6 starts admitted against a cap of 2.
2. **Mixed-reason gap coalescing discarded known counts.** Merging `Some(5)` with `None`
   produced `None`. Only some gap reasons count discrete frames — a hidden target or a
   stopped capture spans time without countable drops — so an absent count means
   "contributes nothing", not "unknown total". The old rule threw away the one hard number
   in the pair and under-reported loss. The function's own comment claimed the count "stays
   exact", which was false in exactly this case.
3. **`FrameRejected` count asymmetry.** Reader-side rejection recorded `Some(1)`;
   worker-side recorded `None` for the identical single-frame loss, so where the rejection
   happened to be detected changed how it was reported.

### `QualificationLifecycle::Drop` browser termination — not needed (no code change)

Considered and rejected as redundant coupling. PDEATHSIG keys on death of the *forking
thread*. The eval harness runs on a current-thread Tokio runtime, so the thread that forks
Chrome is the same thread the run occupies; when a panicking run unwinds and that thread
exits, the signal fires. Unwind is therefore already covered by the same mechanism that
covers SIGKILL. Wiring browser ownership into a lifecycle that currently owns only the
fixture server, profile root, and browser lock would add coupling for no additional coverage.

## Implementation Units

| # | Change | Files |
|---|---|---|
| 1 | PDEATHSIG in child pre-exec + orphan-before-exec guard | `crates/krometrail-cdp/src/launcher/process.rs:1-11, 261-305` |
| 2 | Orphan regression test | `crates/krometrail-cdp/tests/process_ownership.rs:105-152` |
| 3 | Remove tautological `ConnectionLost` match | `crates/krometrail-cdp/tests/session_supervision.rs` (deleted `cancellation_input_is_typed_at_the_supervision_boundary`) |
| 4 | Replace `Arc` self-comparison with recipient assertions | `src/app.rs:1023-1041`, `src/progressive/service.rs:24-30`, `src/debug_bundle/service.rs:85-91` |
| 5 | Real-Chrome root drop order + root-removal assertion | `crates/krometrail-cdp/tests/chrome_session_real.rs:59-68, 152-161` |
| 6 | Cross-layer MCP cancellation regression | `crates/krometrail-mcp/src/server.rs:1528-1746` |
| 7 | Remove proven-redundant `_assert_send_sync` | `crates/krometrail-cdp/src/capture/pipeline.rs` |
| 8 | Fill clear-verification regression | `crates/krometrail-cdp/src/control/tests.rs:573-618` |
| 9 | Active-stream admission reservation | `crates/krometrail-cdp/src/capture/mod.rs:202-209, 249`, `pipeline.rs:801-840, 902-950` |
| 10 | Shared gap count aggregation + `FrameRejected` normalization | `crates/krometrail-cdp/src/capture/pipeline.rs:1184-1190, 1662-1712` |

## Testing

Each new test was verified to fail against the pre-fix behavior, not merely to pass:

- `managed_child_dies_when_its_launcher_never_runs_teardown` — with PDEATHSIG disabled:
  `browser survived a launcher that ran no teardown; parent-death signal not armed`.
  Guarded against vacuous passes by requiring a live child before the launcher thread exits.
- `concurrent_starts_cannot_exceed_the_active_stream_cap` — pre-fix:
  `the cap must hold across concurrent starts; 6 of 6 were admitted`. Uses a transport that
  yields during subscription so the starts genuinely interleave; an immediately-resolving
  transport would run each start to completion and never reproduce the race.
- `client_cancellation_reaches_the_browser_port_and_spares_other_requests` — drives a real
  JSON-RPC `notifications/cancelled` while a fake `BrowserSessionPort::execute` is parked on
  the request's own signal. Asserts the token reaches `BrowserOperationContext`, the caller
  sees `cancelled`, and a concurrent request's context is untouched and succeeds.
- `fill_replace_fails_actionably_when_clearing_is_swallowed` — asserts the failure names the
  unclearable field and that `Input.insertText` is never dispatched onto surviving contents.
- `coalescing_mixed_count_bearing_reasons_keeps_every_known_loss` — mixed merge keeps 4;
  same-reason merge accumulates to 6.

Real-Chrome opt-in suite run against actual Chrome (`KROMETRAIL_REAL_CHROME_TESTS=1`),
7 passed — this exercises the PDEATHSIG launch path end to end.

## Risks

- **PDEATHSIG is thread-scoped, not process-scoped.** Correct here because the product forks
  from a current-thread Tokio runtime on the main thread. If browser launch ever moves to a
  pooled thread — notably `spawn_blocking`, whose threads retire after an idle timeout — the
  browser would be killed while still wanted. This constraint is documented at the call site
  and is the single most important thing to re-check if launch composition changes.
- **PDEATHSIG covers the direct child only.** Chrome's own helpers are grandchildren; they are
  reaped because Chrome propagates parent death to its zygote, not because of our signal. The
  process-group kill remains the mechanism for the cooperative path.
- **`ExecutableFileBusy` retry budget (~15 ms)** is sized for the fork/exec window, not for a
  package manager holding the binary open. Discovery would report `browser_not_found` in that
  window.
- **Two mutexes in capture admission.** `pending_starts` is only ever locked while `streams` is
  held, which fixes the lock order. Any future code that takes `pending_starts` first would
  introduce a deadlock; the invariant is documented on the field.
- **Worker-side rejection still does not call `record_dropped`,** so the `statistics` dropped
  counter remains asymmetric between reader and worker paths even though the gap count is now
  normalized. Deliberately out of scope here (it changes a separate observable counter); worth
  a follow-up item.
- **Second observed flake was not reproduced** — see below. It is recorded, not silenced.

## Second observed flake — not reproduced, not silenced

`krometrail-mcp`'s `successful_temporal_bundle_exposes_canonical_artifact_resource_end_to_end`
was observed failing twice under heavy concurrent load earlier in this cycle, with assertions
concerning inline image counts in a bundle response.

It did **not** reproduce here: 30 full `krometrail-mcp` lib-suite runs at `--test-threads=64`
under 16 concurrent CPU hogs, all green (91 tests per run). Combined with the previously
recorded 7 full-workspace runs, 8 stress runs, and 3 pristine-HEAD runs, that is a substantial
non-reproduction.

No `#[ignore]` and no assertion weakening was applied. Stating plainly: the cause is unknown.
The shared-cause hypothesis with the discovery flake was examined and **does not hold** — the
discovery flake was ETXTBSY under concurrent fork, which is specific to spawning executables
and has no bearing on inline image counts in a bundle response. There is no evidence of a
common mechanism, so "two load-sensitive flakes" should not be treated as one pattern. If it
recurs, capture the full assertion output; an inline-image count is deterministic given the
request, so a genuine failure most likely indicates shared state or a budget/limit that is
sensitive to elapsed time under load.

## Acceptance

- [x] Browser processes cannot outlive a SIGKILLed harness (PDEATHSIG, regression-tested).
- [x] Flaky discovery test root-caused — already fixed upstream; verified at HEAD, item corrected.
- [x] Tautological assertions removed; replaced with real-contract assertions where one existed.
- [x] Each adjudicated item reached a terminal state with recorded rationale.
- [ ] Bundle-resource flake remains unexplained and explicitly open.
