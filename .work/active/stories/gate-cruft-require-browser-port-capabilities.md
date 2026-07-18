---
id: gate-cruft-require-browser-port-capabilities
kind: story
stage: review
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Require new browser-port capabilities explicitly

## Confidence
Medium

## Category
compatibility shim

## Location
`crates/krometrail-core/src/ports/browser.rs:544`

## Evidence

New managed-profile and managed-download capabilities provide empty/not-found trait defaults, allowing adapters and test doubles to omit the capabilities silently even though workspace crates are unpublished internals.

## Removal

Make the new trait methods required and add explicit empty/not-found behavior only to adapters or fakes where it is intentional, restoring compiler-enforced adapter completeness.

## Acceptance criteria

- `BrowserConnector::managed_profiles`, `BrowserSessionPort::read_managed_download`, and `ChromeLauncher::managed_profiles` have no default implementation.
- Every production adapter, delegating wrapper, and test fake implements the required capability explicitly.
- Intentional fake behavior is locally visible as an empty inventory or stable not-found result rather than inherited silently.
- Workspace check and Clippy prove adapter completeness.

## Implementation plan

- Remove the three compatibility defaults from the core/CDP traits.
- Add explicit implementations to every workspace adapter and fake, preserving each test's intended behavior.
- Let compiler errors identify any missed implementation.

## Implementation notes

- Made managed-profile inventory and managed-download reads required on the core browser ports, and managed-profile inventory required on the CDP launcher port.
- Kept production/delegating implementations explicit and added intentional empty/not-found behavior to every affected test adapter.
- Used the workspace compiler and Clippy gates to prove no implementation can inherit the former fallback silently.

## Validation

- `cargo test --workspace --all-targets --locked`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
