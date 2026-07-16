---
id: story-fix-ci-manual-noop-wakers
kind: story
stage: done
tags: [bug, infra, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-16
updated: 2026-07-16
---

# Replace manual no-op test wakers rejected by Clippy

## Symptom

GitHub Actions Rust CI runs `cargo clippy --workspace --all-targets --locked -- -D warnings` and fails both the stable and Rust 1.85 jobs with `clippy::manual_noop_waker` in `crates/krometrail-core/src/ports/mod.rs`. The distribution job is consequently skipped. Equivalent manual wakers also remain in two store test surfaces and would fail once compilation reaches them.

## Root cause

Three test helpers predate the Rust 1.85 `Waker::noop()` API and manually implement `Wake` using an `Arc`. Current Clippy recognizes that shape as `manual_noop_waker`; because CI promotes warnings to errors, the moving stable lint set made previously accepted test code fail the quality gate.

## Fix approach

Use the standard-library `Waker::noop()` implementation in every affected test surface and remove imports/types that become unnecessary. This keeps the MSRV contract intact because `Waker::noop()` stabilized in Rust 1.85, while avoiding a lint suppression that would preserve obsolete code.

## Regression test

The existing CI contract is the regression guard: run `cargo clippy --workspace --all-targets --locked -- -D warnings` under both stable and Rust 1.85. The failure is static lint compatibility rather than runtime behavior, so no additional behavioral test is warranted.

## Implementation notes

- **Execution capability:** direct inline repair. The failure was isolated to three test-only no-op waker implementations with one standard-library replacement and no public contract impact.
- **Files changed:** `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-store/src/segments/writer.rs`, and `crates/krometrail-store/tests/sqlite_qualification.rs`.
- **Regression guard:** replaced all manual implementations found across Rust test sources; no `NoopWaker` or manual `Wake` implementation remains.
- **Confirmation:** formatting and diff checks passed; stable and Rust 1.85 Clippy passed with warnings denied; complete stable and Rust 1.85 workspace test suites passed; distribution contract checks passed; the original Clippy failure no longer reproduces under the exact local commands.
- **Adjacent issues parked:** none.

## Review

- **Mode:** bounded inline standalone-story review; no independent or cross-model reviewer ran.
- **Verdict:** approve.
- **Correctness:** `Waker::noop()` preserves the prior no-op wake behavior and is available at the declared Rust 1.85 MSRV. Every manual implementation was replaced, addressing the lint root cause rather than suppressing it.
- **Tests:** the exact warnings-denied Clippy command and complete workspace tests passed on stable and Rust 1.85; distribution checks also passed. A new runtime test would not add confidence for this static lint regression.
- **Design and compatibility:** the change removes test-only boilerplate, introduces no abstraction, and changes no public API, persisted format, schema, runtime behavior, or foundation assertion.
- **Security:** not applicable; no production input, process, filesystem, network, or secret handling changed.
- **Findings:** no blockers, important findings, or nits.
