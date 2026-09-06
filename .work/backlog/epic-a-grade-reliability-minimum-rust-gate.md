---
id: epic-a-grade-reliability-minimum-rust-gate
kind: feature
stage: done
tags: [infra, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: 1.6.3
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
- **Readiness:** Implemented, independently reviewed, and qualified for release 1.6.3. Publication itself is tracked by the release transaction.

## Original evidence (before the fix)

- Cargo.toml:5,24 — declared rust-version 1.85
- rust-toolchain.toml — stable directory toolchain
- .github/workflows/ci.yml:73 — unqualified cargo in rust-msrv job
- https://github.com/dtolnay/rust-toolchain/blob/3230091d7ef9483f601620abacc570c22cf44d22/action.yml#L94-L97 — action sets rustup default

## Acceptance criteria

- [x] Choose and document the actual minimum: Rust 1.88, selected after explicit Linux locked workspace/all-targets check and tests (receipt below), not inferred from syntax alone.
- [x] CI logs the effective rustc and cargo identities and explicitly selects the intended compiler for every minimum-version gate.
- [x] The final 1.6.3 candidate passed explicit Rust 1.88.0 locked workspace/all-targets check/tests on Linux and the separate stable formatting/Clippy gates.
- [x] A regression fixture or workflow contract test catches reintroduction of ambiguous toolchain selection; installation success alone is not a passing compatibility check.

## Implementation direction and boundaries

Prefer explicit cargo +<version> or equivalent verified selection. Keep stable developer tooling distinct from the supported-minimum build contract.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Execution preflight — 2026-09-05

The parent inspected the current CI and local toolchains while the result-delivery, giant-page, release-ownership, and sequence-provenance units were running. Both minimum-version CI steps and ordinary local cargo commands remain ambiguous without explicit compiler selection; local unqualified rustc/cargo report Fedora 1.96.1. Installed rustup toolchains include 1.85.0 and 1.88.0, so a candidate minimum can be tested without a toolchain download.

An offline locked Cargo metadata inventory succeeded. The highest declared dependency requirement in the complete graph was `wasip2` 1.0.4+wasi-0.2.12 at Rust 1.87.0; other leading entries reported 1.85. This inventory includes target-specific dependencies and is not evidence that the Linux build requires 1.87 or that the workspace compiles on 1.88. Source let chains independently rule out the current 1.85 declaration as-is. Treat 1.88 as a candidate to verify, not a selected minimum; run explicit-toolchain locked gates before changing the promise.

A candidate-only check subsequently passed at `c49a42f75ca9972092b9f07a9b39e3da61cba891`: explicitly selected `rustc 1.88.0 (6b00bc388 2025-06-23)` and `cargo 1.88.0 (873a06493 2025-05-10)` completed `cargo check --workspace --all-targets --locked --offline` on Linux. The command held the shared build lock and used its own temporary target; removal was verified afterward. This establishes compilation for that revision, not tests, lint/format policy, other platforms, or the final integrated revision. No minimum-version declaration or workflow was changed.

The release owner's shared CI/fixture surface became available for the bounded implementation below. Preserve stable formatting/tooling policy separately from minimum compiler compatibility. Any custom compiler-specific build directory created for qualification has an owner and must be removed at completion; do not repeatedly evict the active workers' shared stable build cache by alternating compiler versions there.

## Bounded release prerequisite — 2026-09-05

Selected policy: root package and workspace `rust-version = "1.88"`. The Linux `rust-msrv` job pins `MSRV_TOOLCHAIN: 1.88.0`, installs it with `rustup toolchain install`, and uses `rustup run "$MSRV_TOOLCHAIN"` for both identity commands and every check/test invocation. This bypasses the repository's stable directory-toolchain selection. Stable alone owns rustfmt and Clippy: formatter output and lint rules evolve separately from the minimum compiler's ability to compile and test the workspace. No attempt is made to force old formatting rules onto current sources.

Parent qualification receipt (supplied to this implementation, not rerun here): at `90085cba`, Linux full `cargo check --workspace --all-targets --locked` and `cargo test --workspace --all-targets --locked` passed with explicitly selected `rustc 1.88.0 (6b00bc388 2025-06-23)` and `cargo 1.88.0 (873a06493 2025-05-10)`, including explicit `RUSTC`/`RUSTDOC`. Log: `/tmp/krometrail-release-msrv.log`, exit 0, 2,186 lines. The parent's owned temporary target was removed on exit. This supports the Linux minimum selection, not another platform or optional-feature qualification.

`tests/minimum-rust-workflow.test.ts` parses actual CI YAML and Cargo TOML using Bun built-ins, checks metadata agreement and exact executable minimum-job steps, and rejects wrong compiler/installer selection, unqualified check/test, missing compiler/Cargo identities, metadata drift, step compiler overrides, and ignored test failures. It also preserves stable-only formatting/Clippy gates. The fixture is wired into the existing distribution suite; it installs no dependencies and does not compile code to inspect a manifest.

Bounded local verification: `bun test tests/minimum-rust-workflow.test.ts` — 10 pass, 0 fail (Bun 1.3.14); `bash -n tests/distribution-static.sh` and `git diff --check` passed. No build directories were created or shared target caches touched. README and development instructions now state the selected minimum and Linux qualification boundary; historical research and sample evidence retain their original identities.

Pending parent work: independent review, final integrated stable gates and full distribution fixtures, documentation regeneration/build (`docs/public/llms-full.txt` was deliberately not edited), and final release reconciliation. No version bump, tag, push, or publication is part of this commit; product and temporal-vision versions remain unchanged.

## Final release qualification

The parent relaxed the workflow fixture's unrelated checkout/cache/presentation snapshots while retaining compiler-selection, identity and failure-propagation checks; 11 fixture cases pass. Raising the declaration enabled Clippy's Rust-1.88-compatible suggestions, so the candidate also includes machine-applicable let-chain and nonzero-divisibility rewrites. Independent review accepted their evaluation order, lock/drop scopes, error paths, and compiler support. The reviewed raw Rust/fixture diff SHA-256 is `f4136328d517b2cd7cc8b7b406116714b36de20de90a8f7279017a3232b89a22`.

The final candidate passed stable formatting, wire schemas, locked workspace/all-targets check/tests and warning-free Clippy; full distribution/installer/plugin fixtures; documentation regeneration/build; explicit Rust 1.88.0 locked workspace/all-targets check/tests; and temporal-vision 0.2.0 package verification with upload aborted by `--dry-run`. Receipts: `/tmp/krometrail-1.6.3-final-release-gates.log`, `/tmp/krometrail-1.6.3-distribution-msrv.log`, and `/tmp/krometrail-1.6.3-msrv-package-final.log`. The first two logs contain initial failures that were resolved by the subsequent receipts: sibling marketplace alignment and allowing the dry-run's registry metadata lookup. The compiler-specific target was removed on exit. Shared build cache survives. Optional qualification-support compilation and live-browser coverage are not claimed.
