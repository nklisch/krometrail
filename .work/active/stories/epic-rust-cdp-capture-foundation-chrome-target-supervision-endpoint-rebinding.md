---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-endpoint-rebinding
kind: story
stage: done
tags: [browser, security, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-real-reconnect]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Pin loopback resolution and refresh HTTP endpoints on reconnect

## Origin

Adversarial feature review confirmed two material endpoint defects: `localhost` accepted mixed public/loopback resolution and later re-resolved the hostname, and reconnect reused the initial browser WebSocket URL instead of refreshing `/json/version` for an HTTP attach endpoint whose browser identity/path may rotate.

## Scope

Resolve endpoint hostnames exactly once through an injectable resolver, reject empty or any mixed/non-loopback address set, and pin HTTP discovery plus WebSocket dialing to a validated loopback socket address without resolver TOCTOU. Preserve redacted authority/display and original endpoint kind. On every reconnect attempt, re-resolve eligible HTTP origins through bounded `/json/version`, revalidate/pin the returned browser WebSocket endpoint, and use the fresh path; direct WebSocket attaches remain direct. Extend the real fault proxy to rotate WebSocket paths and require a second version request.

## Acceptance criteria

- [x] Mixed, public, changing, empty, and credential-bearing resolution fails before network side effects; pinned loopback addresses are used for discovery and transport.
- [x] HTTP reconnect performs a fresh `/json/version` request and succeeds when the real browser WebSocket path rotates.
- [x] Direct WebSocket attach behavior remains explicit and validated without fabricating HTTP refresh.
- [x] Deterministic resolver/rebinding and real Chrome tests pass with zero resources; no screencast code lands.

## Implementation notes

- `LocalCdpEndpoint` now preserves `LocalCdpEndpointKind`, original HTTP/WebSocket protocol URLs, and separate validated loopback `SocketAddr` pins. `EndpointResolver` is injectable; empty, mixed, and non-loopback results are rejected before network I/O.
- HTTP discovery writes the original authority in `Host` while dialing the pinned address. The cdpkit adapter receives a crate-private dial URI that swaps only the authority for the pinned numeric address and preserves the WebSocket scheme/path. Direct WebSocket endpoints do not refresh HTTP discovery.
- HTTP-origin reconnects perform fresh `/json/version` discovery on every attempt, reuse the pinned HTTP address, reuse a validated WebSocket address for unchanged authorities, and accept rotated paths. The real fault proxy rejects stale paths, rotates after the second version request, and the real-Chrome test asserts a second physical connection.
- Added deterministic empty/mixed/rebinding/TOCTOU and HTTP Host/path-refresh coverage. No screencast behavior was added.
- Verification: workspace tests, no-default-features check/tests, workspace clippy with and without defaults, and the opt-in cdpkit spike suite (101 tests) passed. Three repeated real-Chrome reconnect runs and the complete real-Chrome session suite (5 tests) passed with profile/process cleanup assertions.
- Exact implementation commit: `43077740b9b66c372f4cf199858ac49efc967c71`.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane boundary review reran 56 focused tests and denied-warning clippy; verified all-address loopback validation, pinned socket dialing, HTTP refresh with rotated WebSocket path, direct-WS semantics, real physical reconnect, and zero resource/screencast leakage. Verdict: Approve - story verified by implement; fast-lane advance.
