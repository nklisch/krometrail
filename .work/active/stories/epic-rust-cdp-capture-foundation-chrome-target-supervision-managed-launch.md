---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Own Chrome discovery, profiles, and managed process lifecycle

## Scope

Implement Unit 3 of the parent design: deterministic Linux/macOS browser discovery, Krometrail-owned named/temporary profile leases, loopback ephemeral-port launch, cancellation-safe child/process-group ownership, endpoint readiness, graceful close, escalation, and exact cleanup rules.

Do not connect cdpkit, supervise targets, auto-relaunch Chrome, touch attached profiles/processes, or add product screencast behavior.

## Required files

- `crates/krometrail-cdp/src/launcher/{mod.rs,discovery.rs,profile.rs,process.rs,startup.rs}`
- `crates/krometrail-cdp/tests/profile_ownership.rs`
- `crates/krometrail-cdp/tests/process_ownership.rs`

## Acceptance criteria

- [ ] Discovery precedence is explicit executable, environment override, platform stable paths, then PATH; results are canonicalized, executable, and deduplicated.
- [ ] Named profile input cannot traverse the managed root and is exclusively leased; reusable profiles survive stop, while only the owning temporary guard deletes its path.
- [ ] Process/profile ownership exists before the first await. Startup cancellation/timeout, graceful close, forced escalation, and drop clean only the held child tree/profile.
- [ ] Launch binds CDP to loopback and never modifies the user's default profile. Headless/gpu/sandbox flags are test configuration rather than product defaults.
- [ ] Deterministic injected filesystem/process/readiness tests cover cancellation boundaries without sleeps and prove attach resources are never cleaned.
