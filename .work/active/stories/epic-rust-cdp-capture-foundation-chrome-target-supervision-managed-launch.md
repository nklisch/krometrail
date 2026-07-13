---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch
kind: story
stage: review
tags: [browser]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Own Chrome discovery, profiles, and managed process lifecycle

## Scope

Implement Unit 3 of the parent design after the transport adapter: deterministic Linux/macOS browser discovery, Krometrail-owned named/temporary profile leases, loopback ephemeral-port launch using transport-owned `LocalCdpEndpoint`, cancellation-safe child/process-group ownership, endpoint readiness, graceful close, escalation, and exact cleanup rules.

Ownership is concrete:

- `LocalCdpEndpoint` is the validated non-owning endpoint value from `endpoint.rs`.
- `ProfileLease` owns the canonical managed path, `ProfileRef::Managed`, exclusive lock, lease kind, and temporary-only cleanup guard for the complete managed session.
- `ManagedChromeProcess` owns the child handle, isolated process-group termination authority, and process-termination stream. It is the only value allowed to kill the managed tree.
- `LaunchedChrome { endpoint, profile, process }` transfers those guards into session supervision; cleanup stops the child before releasing/deleting its profile. Attach mode constructs neither ownership guard.

One shared discovery helper accepts an optional launch-request executable and populates every `BrowserInstallation { executable, source, product, version }`. `ChromeLauncher::installations()` calls it without an explicit request for doctor; `launch()` supplies `LaunchBrowser.executable` so explicit-request precedence remains real. Later connector/doctor code delegates to `installations()` and must not reproduce discovery policy.

Do not supervise targets, auto-relaunch Chrome, touch attached profiles/processes, or add product screencast behavior.

## Required files

- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/launcher/mod.rs` (new)
- `crates/krometrail-cdp/src/launcher/discovery.rs` (new)
- `crates/krometrail-cdp/src/launcher/profile.rs` (new)
- `crates/krometrail-cdp/src/launcher/process.rs` (new)
- `crates/krometrail-cdp/src/launcher/startup.rs` (new)
- `crates/krometrail-cdp/tests/profile_ownership.rs` (new)
- `crates/krometrail-cdp/tests/process_ownership.rs` (new)

`crates/krometrail-cdp/src/lib.rs` is intentionally touched after the transport story to export `launcher`; the dependency chain serializes this shared-file edit.

## Acceptance criteria

- [x] The shared helper orders optional explicit request, environment override, platform stable paths, then PATH; `installations()` omits only the unavailable request candidate and `launch()` includes it. Canonical executable results are deduplicated and fully classify source, product, and bounded-probe version. Electron is not platform-discovered as managed Chrome.
- [x] Named profile input cannot traverse the managed root and is exclusively leased; reusable profiles survive stop, while only the owning temporary guard deletes its path. Attach never creates a `ProfileLease`.
- [x] Process/profile ownership exists before the first await. Startup cancellation/timeout, graceful close, forced escalation, drop order, and transfer into `LaunchedChrome` clean only the held child tree/profile.
- [x] Managed child exit produces the sanitized process-termination signal consumed by supervision; it is distinct from transport close and covered without sleeps.
- [x] Launch binds CDP to loopback and never modifies the user's default profile. Headless/gpu/sandbox flags are test configuration rather than product defaults.
- [x] Deterministic ownership tests cover duplicate profiles, temporary cleanup, child death, escalation, and transfer/drop boundaries; endpoint readiness and cancellation are bounded by configuration/timeout boundaries.
- [x] `browser.discovery.completed`, `browser.launch.started|ready|failed`, and `browser.shutdown.completed|incomplete` tracing contains only the parent's sanitized fields; no full executable/profile paths, command-line secrets, or source/debug error strings appear at info level.

## Implementation notes

- Added `launcher::{discovery,profile,process,startup}` behind the transport-independent `krometrail-cdp` boundary. Discovery is shared by installation listing and explicit launch, with canonical deduplication, Linux/macOS stable paths, environment override, PATH lookup, bounded `--version` probing, and Electron exclusion for platform candidates.
- `ProfileLease` owns a canonical managed path, exclusive lock, managed `ProfileRef`, and temporary-only cleanup. `ManagedChromeProcess` owns the child/process group and sanitizes exit status; `LaunchedChrome` explicitly kills the process before its profile guard can drop. Attach only uses `LocalCdpEndpoint::resolve`.
- Launch selects an OS ephemeral loopback port, starts the isolated process group before awaiting readiness, resolves `/json/version` into the existing `LocalCdpEndpoint`, and applies bounded startup/shutdown timeouts. No session supervision, reconnect, target reduction, or screencast code was added.
- Added integration coverage in `tests/profile_ownership.rs` and `tests/process_ownership.rs`, plus focused launcher unit coverage. Workspace default/no-default checks and tests, the cdpkit spike regression, and default/no-default clippy all pass.
