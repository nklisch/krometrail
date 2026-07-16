---
id: epic-agent-browser-operation-waits-and-batches-qualification-and-wiring
kind: story
stage: done
tags: [browser, agent-ux, testing]
parent: epic-agent-browser-operation-waits-and-batches
depends_on: [epic-agent-browser-operation-waits-and-batches-batch-coordinator]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-14
---

# Wait and Batch Qualification and Wiring

## Checkpoint

Integrate the wait and batch routes through the production connector/supervisor, add a real-Chrome fixture and end-to-end qualification, and close the feature's full verification boundary. This story owns verification and composition wiring only; it does not register MCP tools, add storage, or add temporal query behavior.

## Files

- `crates/krometrail-cdp/tests/waits_and_batches.rs` (new) — real-browser and scripted integration tests.
- `tests/fixtures/browser/waits-and-batches/` (new or existing fixture extension) — delayed state/text, navigation, finite requests, and long-lived connection scenarios.
- `crates/krometrail-cdp/src/session.rs`, `crates/krometrail-cdp/src/control/mod.rs`, and composition exports — only final additive wiring and qualification fixes.

## Acceptance evidence

- Real Chrome demonstrates elapsed, text present/absent, element attached/visible/enabled/editable/checked state, navigation readiness/URL, boolean page condition, and explicitly requested network quiet. The network test demonstrates finite request completion plus the documented long-lived/pre-subscription limitations; no implicit network-idle assertion is added to navigation or interactions.
- Real Chrome demonstrates ordered navigation/interaction/wait batches, default stop, opt-in continuation, skipped steps, optional per-step screenshots, one final live observation, stale-reference failure, cancellation/deadline, and explicit degraded observation.
- Assertions inspect browser state and returned typed contracts: operation kinds, target identity, monotonic timings, child anchors, parent-batch correlation, statuses/outcomes, screenshot cardinality, final observation, stable errors, and no cross-target execution.
- Linux real-Chrome qualification passes. The existing macOS CI path runs deterministic and real-Chrome qualification where available; unavailable evidence is recorded honestly rather than weakened or fabricated.
- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass. The existing browser lifecycle/page-observation/verified-interaction qualifications remain green.
- The story records exact verification commands, browser/platform evidence, any bounded environment limitation, and changed files before advancing directly to `done`.

## Implementation notes

Use the existing standalone browser fixture/test support and Chrome connector. Do not add MCP registration, SQLite/timeline writes, or a second fixture runtime. A real-browser failure is evidence to diagnose, not a reason to weaken an assertion.

## Implementation notes

- Added the dependency-free `tests/fixtures/browser/waits-and-batches/` target with delayed text/element/page state, finite image loading, a deliberately excluded WebSocket attempt, dynamic node replacement, delayed navigation, and batch-visible counter state. The fixture README and test support expose it without introducing a second runtime.
- Expanded `crates/krometrail-cdp/tests/waits_and_batches.rs` with production-port scripted evidence for absolute timeout, cancellation, finite-network tracking, private-id redaction, stop/continue, global deadline and skip reasons, parent correlation, screenshot routing, and exactly-one final observation.
- Added opt-in real-Chrome qualification for elapsed/text present and absent/all element-state/page/navigation/network-quiet waits, stale references, deadline/cancellation, ordered interaction/wait/evaluation and interaction/navigation/wait batches, stop/continue policy, child anchors and `parent_batch`, per-step screenshot success/degradation, browser state, and final live observation.
- Qualification exposed two integration issues and fixed them rather than weakening evidence: independent named network streams could lose a non-selected event future, so three operation-scoped pumps now merge into one bounded receiver and reconcile finish-before-start scheduling; final live-observation component degradation now produces `CompletedWithFailures` without erasing successful children. Child wait timeout remains an ordinary step failure unless the shared batch deadline is actually exhausted.
- Linux real-browser evidence: Google Chrome `149.0.7827.155`; `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test waits_and_batches opt_in_real_chrome_ -- --nocapture --test-threads=1` passed 2 tests in 109.04s. Existing lifecycle/page-observation/verified-interaction real-browser qualification passed 31 tests in 168.61s. macOS was unavailable in this Linux environment; the same opt-in tests remain platform-neutral for the existing macOS path.
- Authoritative gates passed: `cargo fmt --all -- --check`; `cargo check --workspace --all-targets --locked`; `cargo test --workspace --all-targets --locked` (403 passed across 38 suites); `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo check -p krometrail-cdp --no-default-features --all-targets --locked`; `cargo test -p krometrail-cdp --no-default-features --all-targets --locked` (35 passed across 18 suites).
- No MCP registration, storage/timeline write, temporal query, implicit network-idle policy, cross-target ordering, or replay behavior was added.
