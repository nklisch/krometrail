---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-endpoint-rebinding
kind: story
stage: implementing
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

- [ ] Mixed, public, changing, empty, and credential-bearing resolution fails before network side effects; pinned loopback addresses are used for discovery and transport.
- [ ] HTTP reconnect performs a fresh `/json/version` request and succeeds when the real browser WebSocket path rotates.
- [ ] Direct WebSocket attach behavior remains explicit and validated without fabricating HTTP refresh.
- [ ] Deterministic resolver/rebinding and real Chrome tests pass with zero resources; no screencast code lands.
