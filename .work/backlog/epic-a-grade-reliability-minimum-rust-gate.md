---
id: epic-a-grade-reliability-minimum-rust-gate
kind: feature
stage: backlog
tags: [infra, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Make the declared minimum Rust version genuinely compile and gate CI

## Outcome and priority

Actual Rust 1.85 rejects current let chains with E0658. Installing a default 1.85 toolchain does not override this repository's stable toolchain file, so a named MSRV job can test stable instead. The review's first apparently green check was not accepted until compiler identity was explicitly forced.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Reproduced with explicit Rust 1.85 compiler selection; CI toolchain-selection flaw code-traced.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- Cargo.toml:5,24 — declared rust-version 1.85
- rust-toolchain.toml — stable directory toolchain
- .github/workflows/ci.yml:73 — unqualified cargo in rust-msrv job
- https://github.com/dtolnay/rust-toolchain/blob/3230091d7ef9483f601620abacc570c22cf44d22/action.yml#L94-L97 — action sets rustup default

## Acceptance criteria

- [ ] Choose and document the actual minimum: restore source/dependency support for 1.85 or raise the declaration consistently. Do not infer the replacement minimum from let-chain syntax alone.
- [ ] CI logs the effective rustc and cargo identities and explicitly selects the intended compiler for every minimum-version gate.
- [ ] The declared-minimum locked workspace/all-targets check, tests, and applicable lint/format policy pass on the intended compiler.
- [ ] A regression fixture or workflow contract test catches reintroduction of ambiguous toolchain selection; installation success alone is not a passing compatibility check.

## Implementation direction and boundaries

Prefer explicit cargo +<version> or equivalent verified selection. Keep stable developer tooling distinct from the supported-minimum build contract.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Execution preflight — 2026-09-05

The parent inspected the current CI and local toolchains while the result-delivery, giant-page, release-ownership, and sequence-provenance units were running. Both minimum-version CI steps and ordinary local cargo commands remain ambiguous without explicit compiler selection; local unqualified rustc/cargo report Fedora 1.96.1. Installed rustup toolchains include 1.85.0 and 1.88.0, so a candidate minimum can be tested without a toolchain download.

An offline locked Cargo metadata inventory succeeded. The highest declared dependency requirement in the complete graph was `wasip2` 1.0.4+wasi-0.2.12 at Rust 1.87.0; other leading entries reported 1.85. This inventory includes target-specific dependencies and is not evidence that the Linux build requires 1.87 or that the workspace compiles on 1.88. Source let chains independently rule out the current 1.85 declaration as-is. Treat 1.88 as a candidate to verify, not a selected minimum; run explicit-toolchain locked gates before changing the promise.

A candidate-only check subsequently passed at `c49a42f75ca9972092b9f07a9b39e3da61cba891`: explicitly selected `rustc 1.88.0 (6b00bc388 2025-06-23)` and `cargo 1.88.0 (873a06493 2025-05-10)` completed `cargo check --workspace --all-targets --locked --offline` on Linux. The command held the shared build lock and used its own temporary target; removal was verified afterward. This establishes compilation for that revision, not tests, lint/format policy, other platforms, or the final integrated revision. No minimum-version declaration or workflow was changed.

Queue implementation after the release owner's shared CI/fixture surface is available. Preserve stable formatting/tooling policy separately from minimum compiler compatibility. Any custom compiler-specific build directory created for qualification has an owner and must be removed at completion; do not repeatedly evict the active workers' shared stable build cache by alternating compiler versions there.
