# Layered CDP Qualification

Use scripted transports for deterministic protocol tests, loopback proxies for fault injection, and opt-in real Chrome only for qualification.

## Rationale

Protocol behavior remains fast and deterministic by default while ownership, reconnect, and real wire behavior still receive explicit browser evidence.

## Examples

- `crates/krometrail-cdp/tests/support/chrome.rs:3` — qualification support is feature-gated with a test support boundary.
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs:62` — `ScriptedCdp` supplies deterministic protocol behavior.
- `crates/krometrail-cdp/tests/support/cdp_proxy.rs:26` — a loopback proxy faults the transport without turning it into browser process failure.
- `crates/krometrail-cdp/tests/session_supervision.rs:492` — real Chrome reconnect qualification is explicit opt-in, locked, and profile-scoped.

## When to Use

Use for browser transport, reconnect, ownership, routing, and protocol qualification.

## When Not to Use

Pure core-domain tests should not depend on transport infrastructure or Chrome.

## Common Violations

- Making Chrome mandatory for default tests.
- Testing only mocks with no real-browser lane.
- Killing Chrome instead of faulting the transport boundary.
- Polling with sleeps.
- Omitting profile locks or cleanup.
