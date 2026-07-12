# Rust CDP Transport Research — 2026-07-12

## Context and method

This research grounds `epic-rust-cdp-capture-foundation-cdp-transport-gate`. Krometrail needs a browser-level Chrome DevTools Protocol (CDP) connection with flat target sessions, typed control operations, raw protocol escape hatches, and a screencast event path that Krometrail—not the library—can supervise and bound.

Evidence was frozen on 2026-07-12 from crates.io metadata, published/repository source at `cdpkit` 0.4.0 (`15dd5e6d`) and `chromey` 2.52.0 (`6eeca5e8`), their CI/tests, and their public GitHub issues. Strict schema-v2 real-Chrome qualification selected exact published `cdpkit` 0.4.0 after all 13 unchanged gates passed on Linux and macOS from gate revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2`. That selection remains a replaceable adapter decision, not production lifecycle or capture implementation.

Caller-aware research ran direct-read only: the caller prohibited subagents and questions, and the bounded candidate/source surfaces did not warrant delegation. Reversible uncertainties are therefore converted into explicit spike gates below.

## Project gates

A viable transport must provide:

1. compatible maintenance, license, and minimum supported Rust version (MSRV);
2. typed Page, Target, Runtime, Accessibility, and Input commands;
3. browser-level commands over the browser WebSocket;
4. flat target attachment and correct `sessionId` command/event routing;
5. arbitrary command send and named raw event subscription without waiting for regenerated bindings;
6. `Page.startScreencast`, `Page.screencastFrame`, and prompt `Page.screencastFrameAck`;
7. an explicit protocol-drift strategy;
8. connection loss surfaced cleanly so Krometrail owns reconnect and target restoration; and
9. deterministic fake-WebSocket tests plus real-browser qualification.

## Evidence matrix

| Gate | `cdpkit` 0.4.0 | `chromey` 2.52.0 | Minimal owned transport |
|---|---|---|---|
| Health / license / MSRV | MIT OR Apache-2.0; Rust 1.75. Released 2026-06-26, 8 releases since 2026-02, but only 406 total downloads, one contributor, one star: active but very young and single-maintainer. | MIT OR Apache-2.0; Rust 1.70. Released 2026-07-03, 290k total downloads, 50 stars, mostly one maintainer: active with materially more field use. | Krometrail's MIT / Rust 1.85 policy; `tokio-tungstenite` 0.30.0 itself requires Rust 1.85. Maintenance burden transfers to Krometrail. |
| Typed required domains | Source-generated modules expose Page, Target, Runtime, Accessibility, Input, and Browser commands, including `GetFullAxTree`, `DispatchMouseEvent`, `Evaluate`, `GetVersion`, and `SetAutoAttach`. | Generated `spider_chromiumoxide_cdp` commands cover the same domains; public `Browser::execute<T: Command>` and `Page::execute<T: Command>` are typed. | Must generate a pinned, narrow contract from official protocol JSON/PDL. Hand-copying a growing command set is not acceptable. |
| Browser-level connection | `CDP` is explicitly browser-level; typed and raw commands sent through `&CDP` omit `sessionId`. Direct browser-WebSocket and `/json/version` discovery APIs exist. | `Browser::connect` discovers `/json/version`; `Browser::execute` submits commands without a session. | Straightforward envelope routing, but discovery, limits, timeouts, close behavior, and pending-request cleanup become owned code. |
| Flat sessions / routing | `AttachToTarget` and `SetAutoAttach` default `flatten: Some(true)` in 0.4.0. `Session`/`OwnedSession` insert `sessionId`; event listeners filter it. Source and mock tests pass, but real target ordering is unproved. | Handler initialization sets `flatten(true)` and maintains session-to-target maps. It has substantial mock and real-Chrome tests, but issue #8 documents a prior target/session ordering hang fixed in March 2026. | Full control and best observability, but Krometrail must correctly own correlation, detach races, and pending calls. |
| Raw command | `Sender::send_raw(method, Value)` works at browser or session scope and returns `Value`. | No direct method-string API, but public, unsealed `Command` and `Method` traits allow a local generic value command. This is an adapter shim, not a first-class API. | Native by design. |
| Raw event subscription | `Sender::event_stream::<Value>(name)` subscribes before dispatch and session-filters, but returns only `params`; there is no wildcard/full-envelope stream. Typed decode failures are logged and skipped. | `CustomEvent` supports named event subscriptions and `CdpEvent::Other`, but events pass through the generated event decoder first. Issue #5 is real protocol-drift evidence: one newly observed enum value caused WebSocket event deserialization failures until bindings changed. No wildcard/full-envelope public stream. | Can preserve `{method, sessionId, params}` before optional typed decoding and is the strongest drift boundary. |
| Screencast / ack | Typed `StartScreencast`, `ScreencastFrame`, and `ScreencastFrameAck(i64)` signatures exist. Per-subscription channels are intentionally unbounded since 0.3.2, so the spike must prove the reader can ack before handing off to Krometrail's bounded queue without unbounded buildup. | Typed event and convenience `Page::start_screencast` / `ack_screencast` exist. Internal event listeners are also unbounded. Real sustained screencast behavior is not covered by the inspected tests. | Can make ack-before-bounded-handoff explicit and avoid an intermediate unbounded subscriber, at the cost of transport code. |
| Protocol drift | Generated from the official tip-of-tree JSON, but 0.4.0 records only protocol version `1.3`, not the source commit/date. Raw commands and `Value` events reduce binding lag; provenance remains weak. | Generated PDL is checked in but published provenance is likewise not pinned. Issue #5 shows drift has broken event decoding in practice. | Pin the official `devtools-protocol` commit in generated output and keep raw envelopes authoritative; regeneration and compatibility fixtures become Krometrail responsibilities. |
| Reconnect ownership | No transparent reconnect. `CloseReason`, `is_closed`, pending-call draining, and closed event streams give Krometrail a usable supervision signal. This matches the architecture. | Retry settings cover initial connection attempts. Runtime connection loss ends the handler; Krometrail still needs to reconstruct browser/target state. | Explicitly Krometrail-owned and easiest to model, but also easiest to get subtly wrong. |
| Testability | Compact `connect_ws` boundary and mock-WebSocket tests cover raw send, session routing, event filtering, disconnect, timeout, and concurrency. Release CI tests only mocks; no real Chrome gate was found. | Rich fake-CDP and real-Chrome integration suites run under Xvfb in CI. The larger handler has more ordering/state behavior to understand and bypass. | Best if designed around an injected connector/duplex; highest initial test-writing cost. |

## Source-level API shape

`cdpkit` has the most direct fit for Krometrail's adapter boundary:

```rust
use cdpkit::{page, target, CDP, Sender};
use futures::StreamExt;

let cdp = CDP::connect_ws(browser_ws_url).await?;
let attached = target::methods::AttachToTarget::new(target_id).send(&cdp).await?;
let session = cdp.owned_session(attached.session_id);

let mut frames = page::events::ScreencastFrame::subscribe(&session);
page::methods::StartScreencast::new().send(&session).await?;
while let Some(frame) = frames.next().await {
    page::methods::ScreencastFrameAck::new(frame.session_id)
        .send(&session)
        .await?;
    // Only then attempt Krometrail's bounded handoff.
}

let raw = session
    .send_raw("Page.getLayoutMetrics", serde_json::json!({}))
    .await?;
```

The important limitation is equally concrete: `event_stream::<Value>("Domain.event")` gives raw event parameters for one named event, not every incoming raw envelope. The spike must not accidentally claim broader escape-hatch semantics.

## Selection outcome: exact cdpkit 0.4.0

### Qualification evidence

The retained version-1 reports and decision are historical inputs only. The accepted version-2 reports use the exact same gate revision/configuration/fixture and include canonical RSS fields, observed lifecycle fields, and bound candidate-contract results:

- `docs/evidence/cdp-transport/v2/cdpkit-linux.json` — SHA-256 `0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148`; Linux x86_64, Chrome 149.0.7827.155.
- `docs/evidence/cdp-transport/v2/cdpkit-macos.json` — SHA-256 `c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998`; macOS aarch64, Chrome 149.0.7827.201, hosted run `29202919716`.
- `docs/evidence/cdp-transport/v2/decision.json` — generated by `decide_from_files`; SHA-256 `0288aa9a379b467042409ac27056107b443ea0d91bd21fc4fc8c2beae44c075b`.

The version-2 Rust decision function validates the complete 13-gate registry, exact canonical RSS fields, immutable gate implementation/configuration/fixture identity, exact candidate/version/checksum, Linux/macOS identity, redaction, observed lifecycle fields, and every measured gate contract. Its platform-labelled output binds each report's candidate-contract trace/hash/results and selects `adopt_cdpkit` without waivers.

### 1. Exact cdpkit 0.4.0

The generated v2 decision selects exact `cdpkit` 0.4.0: its source API maps cleanly to Krometrail's replaceable transport adapter and leaves reconnect ownership in the right layer. The remediation harness derives candidate routing, ordering, detach, close, and rebuild assertions from one observed wire controller. Real-Chrome routing counts are derived from unique correlated command/event tokens. The sustained runs remain continuously drained RSS/counter proxies; they do not prove that cdpkit's hidden unbounded subscriber queue is bounded.

The selection does not broaden cdpkit's API. `event_stream::<Value>(name)` preserves parameters for one named event only; it is not wildcard or full-envelope receive. The subscriber remains unbounded and queue depth remains uninspectable. Krometrail must acknowledge before its bounded handoff, own backpressure and capture-gap policy, and own reconnect/session restoration.

### 2. Selection rules carried forward

- **Adopt `cdpkit` behind `krometrail-cdp::transport` only if every mandatory gate passes unchanged.** A small adapter for domain errors and bounded handoff is expected; patching/forking its routing, event decoder, or lifecycle is a failure.
- **Spike `chromey` with the same harness only if `cdpkit` fails on demonstrated real-browser lifecycle, target ordering, or sustained-capture behavior that chromey's mature handler may solve.** Adopt it only if typed commands, a local generic raw-command wrapper, named raw event subscription, and explicit reconnect ownership all pass without importing its crawling/network-policy behavior into core contracts.
- **Go directly to a minimal owned transport if either candidate loses unknown events before a raw boundary, cannot expose reliable session routing, requires a fork, or obscures prompt ack/backpressure.** The owned fallback should use Tokio plus `tokio-tungstenite`, keep raw envelopes as the source of truth, and generate the supported typed command/event subset from a pinned official protocol revision.
- **Do not weaken a gate to select a library.** If both libraries fail, the evidence justifies the owned cost.

## Implementation constraints

- `cdpkit` is unusually young and single-maintainer; pin exactly `=0.4.0` and keep the production adapter narrow and replaceable.
- Its event subscriber is unbounded. Krometrail's bounded ingestion does not bound memory upstream of that queue; the committed RSS result is a process-level trend proxy, not queue-depth proof.
- Generated protocol version `1.3` is not meaningful provenance by itself. The selected adapter carries forward the explicit lack of a cdpkit source revision.
- Reconnect is a browser-session state transition, not a WebSocket retry. Krometrail must recreate discovery, attachments, domain enablement, screencast state, and gap evidence.
- The spike remains non-default and is not root-wired. Production lifecycle, capture, reconnect, and backpressure implementation belong to later features.

## References

- [`cdpkit` 0.4.0 on crates.io](https://crates.io/crates/cdpkit/0.4.0) — version, license, MSRV, release metadata.
- [`cdpkit` 0.4.0 source](https://github.com/yie1d/cdpkit-rs/tree/15dd5e6d87a03517d5dc2f0b0d49efb90ea2f1ea) — inspected implementation and tests.
- [`cdpkit::Sender` and session APIs](https://github.com/yie1d/cdpkit-rs/blob/15dd5e6d87a03517d5dc2f0b0d49efb90ea2f1ea/cdpkit/src/lib.rs) — typed/raw send, event stream, browser and session handles.
- [`cdpkit` message routing](https://github.com/yie1d/cdpkit-rs/blob/15dd5e6d87a03517d5dc2f0b0d49efb90ea2f1ea/cdpkit/src/inner.rs) — envelope correlation, session routing, raw event params, close cleanup.
- [`cdpkit` 0.4.0 changelog](https://github.com/yie1d/cdpkit-rs/blob/15dd5e6d87a03517d5dc2f0b0d49efb90ea2f1ea/CHANGELOG.md) — flatten defaults, unbounded channels, timeout/close changes.
- [`chromey` 2.52.0 on crates.io](https://crates.io/crates/chromey/2.52.0) — version, license, MSRV, release metadata.
- [`chromey` 2.52.0 source](https://github.com/spider-rs/chromey/tree/6eeca5e8) — inspected handler, protocol bindings, tests, and CI.
- [`chromey` browser API](https://github.com/spider-rs/chromey/blob/6eeca5e8/src/browser.rs) and [page API](https://github.com/spider-rs/chromey/blob/6eeca5e8/src/page.rs) — typed execute, event listeners, screencast/ack.
- [`chromey` issue #5](https://github.com/spider-rs/chromey/issues/5) — real event-deserialization failure from protocol drift.
- [`chromey` PR #8](https://github.com/spider-rs/chromey/pull/8) — real target/session ordering hang and repair.
- [`chromey` issue #4](https://github.com/spider-rs/chromey/issues/4) — typed high-level behavior drift around Runtime contexts.
- [`tokio-tungstenite` 0.30.0](https://crates.io/crates/tokio-tungstenite/0.30.0) — current owned-transport WebSocket candidate and Rust 1.85 MSRV.
- [Official CDP browser protocol](https://chromedevtools.github.io/devtools-protocol/tot/) and [source repository](https://github.com/ChromeDevTools/devtools-protocol) — authoritative evolving protocol definitions.
