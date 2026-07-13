---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-bounded-reconstruction
kind: story
stage: review
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-endpoint-rebinding]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Bound and cancel complete reconnect reconstruction

## Origin

Adversarial feature review confirmed that the attempt timeout covered only connection/setup, leaving serial per-target attachment/effects outside the deadline and unable to observe stop, cancellation, or managed process death.

## Scope

Treat reconnect as one transactional attempt: endpoint refresh, connection/setup, target snapshot, bounded target attachments/visibility/domain restoration, reducer reconstruction, and effect application must fit the attempt deadline and race against stop/cancel/process-death. Bound target count and attachment concurrency explicitly. Do not commit the new connection generation, state, pumps, or published restored events until complete reconstruction succeeds; discard partial work on timeout/cancel. Preserve finite backoff and exact-key identity.

## Acceptance criteria

- [ ] Entire reconstruction is deadline-bound and immediately observes stop/cancel/process death.
- [ ] Target restoration has explicit count/concurrency bounds and cannot grow attempt duration linearly without limit.
- [ ] Partial timeout/failure publishes no committed generation or false restored state; next attempt starts cleanly.

## Implementation notes

- Reconnect reconstruction now owns one deadline/cancellation transaction from HTTP endpoint refresh through connection setup, bounded snapshot validation, parallel target attachment, domain/visibility restoration, reducer staging, and transport effects. It stages state and events and commits the generation, pumps, and restored publication only after every step succeeds.
- `SupervisorConfig` exposes a 64-target default cap and four-way attachment/domain restoration bound. Partial sessions are tracked for globally bounded best-effort detachment before the temporary transport is dropped; exact browser target keys remain the reducer identity.
- Managed process death has a direct notification path into the in-flight transaction, in addition to the existing supervision input. The retry schedule remains finite and endpoint refresh still happens once per attempt.
- Added deterministic cap, many-target concurrency, stalled-command deadline, cancellation, and process-death signal tests. The real Chrome proxy test now repeats three rotating-path reconnects and verifies process/profile cleanup.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo check --workspace --no-default-features --all-targets --locked`
- `cargo test -p krometrail-cdp --features cdp-spike --all-targets --locked`
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test session_supervision opt_in_real_chrome_reconnects_through_a_new_physical_proxy_connection`
- [ ] Deterministic stalled-command/many-target/cancellation tests plus real reconnect tests pass leak-free.
