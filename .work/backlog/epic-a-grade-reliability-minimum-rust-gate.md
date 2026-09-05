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
