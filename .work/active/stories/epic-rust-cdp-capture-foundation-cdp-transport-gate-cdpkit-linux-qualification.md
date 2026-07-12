---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-cdpkit-linux-qualification
kind: story
stage: implementing
tags: [browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Qualify exact cdpkit 0.4.0 on Linux

## Scope

Implement only the disposable `cdpkit` spike adapter and run the shared fake-WebSocket and real-stable-Chrome gates on Linux. Produce schema-valid, sanitized machine-readable Linux evidence. Do not select the production transport, revise core ports, or implement production browser lifecycle/capture.

## Exact files

- `Cargo.lock`
- `crates/krometrail-cdp/src/spike/cdpkit_adapter.rs`
- `crates/krometrail-cdp/src/spike/chrome_harness.rs`
- `crates/krometrail-cdp/src/spike/fixture_server.rs`
- `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs`
- `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs`
- `tests/fixtures/browser/cdp-transport-gate/index.html`
- `tests/fixtures/browser/cdp-transport-gate/animation.js`
- `docs/evidence/cdp-transport/v1/cdpkit-linux.json`

## Requirements

- Adapt exact published `cdpkit` 0.4.0 unchanged. A required fork or routing/decoder/lifecycle patch is a recorded failure, not implementation work.
- Run the shared deterministic scenario suite through `CdpkitTransport` against `ScriptedCdpPeer` before Chrome. Prove browser/session routing, ordering, named raw event params, drift survival, disconnect cleanup, and explicit reconnect/rebuild without sleeps.
- Launch a disposable stable Chrome profile from the spike binary only, serve the committed animated fixture over loopback, and redact binary/profile paths, endpoints, usernames, hostnames, and environment values from committed evidence.
- Exercise typed `Browser.getVersion`, `Page.enable`, `Runtime.evaluate`, `Accessibility.enable` + `getFullAXTree`, harmless `Input.dispatchMouseEvent`, and typed Target flat-session setup. Exercise raw browser and session commands plus named raw event parameters with additive fields. Record the absence of wildcard/full-envelope receive as a limitation, never as a passing capability.
- Prove two page sessions route 100 uniquely tagged commands and 100 same-named events each with zero cross-session deliveries, including event-before-response and detach-during-command.
- Capture for at least 60 seconds and at least 1,000 frames (hard stop 120 seconds). Acknowledge each frame before attempting bounded handoff. Saturate a capacity-1 handoff for at least 10 seconds and 100 handoff attempts; require at least one explicit drop while frame receipt and acknowledgement continue.
- Record receive-to-ack-completion p50/p95/p99/max; pass only when p99 is at most 250 ms and max at most 1,000 ms. Record these as acknowledgement latency proxies, not wire-enqueue timestamps.
- Sample runner RSS once per second. After a 10-second warmup, pass the bounded-memory proxy only when the final 20-second median is no more than 32 MiB above the first 20-second steady-window median and Theil-Sen RSS slope is no more than 8 MiB/minute. Record peak, medians, slope, sample count, and that upstream queue depth is unavailable.
- On forced disconnect, all pending calls/subscriptions must resolve within 1 second; establish a fresh connection and recreate both sessions within 5 seconds with no library reconnect.

## Acceptance criteria

- [ ] The exact lockfile checksum/version, git revision, Rust version, Linux OS/arch, Chrome product/revision/protocol, fixture digest, protocol provenance status, config, per-gate measurements, limitations, and failures are present in schema-valid evidence.
- [ ] All typed/raw/flat-session/drift/disconnect and 60-second/1,000-frame gates are represented individually as pass or fail; no failed requirement is weakened or omitted.
- [ ] Every screencast frame is acknowledged before bounded handoff; deliberate saturation yields explicit handoff drops while acknowledgements continue.
- [ ] Memory claims are limited to the declared RSS trend proxy and do not claim unavailable subscriber queue-depth introspection.
- [ ] Default-feature workspace gates remain green and the spike command/test pass under `cdp-spike-cdpkit`.
- [ ] If cdpkit fails, the evidence names the demonstrated failure. A fallback story is created only then, under this feature, for `chromey` when its mature handler plausibly addresses that failure or for the owned raw-envelope transport when the selection rules require it; no fallback code is added in this story.
