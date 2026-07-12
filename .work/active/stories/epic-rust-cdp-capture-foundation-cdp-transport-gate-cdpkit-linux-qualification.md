---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-cdpkit-linux-qualification
kind: story
stage: done
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

- [x] The exact lockfile checksum/version, git revision, Rust version, Linux OS/arch, Chrome product/revision/protocol, fixture digest, protocol provenance status, config, per-gate measurements, limitations, and failures are present in schema-valid evidence.
- [x] All typed/raw/flat-session/drift/disconnect and 60-second/1,000-frame gates are represented individually as pass or fail; no failed requirement is weakened or omitted.
- [x] Every screencast frame is acknowledged before bounded handoff; deliberate saturation yields explicit handoff drops while acknowledgements continue.
- [x] Memory claims are limited to the declared RSS trend proxy and do not claim unavailable subscriber queue-depth introspection.
- [x] Default-feature workspace gates remain green and the spike command/test pass under `cdp-spike-cdpkit`.
- [x] cdpkit passed the Linux candidate gates, so no fallback story was created; no fallback code was added in this story.

## Implementation notes

- Execution capability: highest-tier direct implementation; the caller prohibited questions and subagents.
- Exact candidate: cdpkit 0.4.0, Cargo.lock checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`.
- Shared scripted-peer candidate test passed with the exact cdpkit adapter and shared scenario registry.
- Real Chrome completed on Linux x86_64 with Chrome 149.0.7827.155: 60.0078 seconds, 3,601 frames received and acknowledged, 3,600 capacity-1 handoff drops, ack proxy p50/p95/p99/max 16.6655/17.0629/17.3118/19.8198 ms, RSS samples 61, first/last medians 8,704,000/8,761,344 bytes, Theil-Sen slope 85,263.7 bytes/minute.
- Evidence: `docs/evidence/cdp-transport/v1/cdpkit-linux.json`; schema generation and validate-and-normalize passed. Protocol source revision is explicitly unavailable because cdpkit reports generated CDP version `1.3` rather than its source commit. Queue depth remains unavailable.
- Verification: workspace fmt/check/test/clippy, cdp-spike check/test/clippy, cdp-spike-cdpkit check/test/clippy, shared candidate contract test, full real-Chrome gate, schema generation, and schema validation passed.
- No production transport, core lifecycle, core-port, fork, or fallback implementation was added. The work-view binary was restored after verification; `.pi/` was left untouched.

## Review (2026-07-12)

**Verdict**: Approve with comments

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran eight cdpkit/spike test targets and candidate-feature clippy, inspected the schema-valid sanitized report, and confirmed every required gate passed. The committed evidence initially referenced the pre-implementation harness commit because evidence was captured before the implementation commit existed; review corrected `source.git_revision` to the implementation commit containing the exact adapter/harness and revalidated normalization. Verdict: Approve - story verified by implement; fast-lane advance.
