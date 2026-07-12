---
name: rust-cdp-transport
description: >
  Versioned Krometrail reference for Rust Chrome DevTools Protocol transport work. Auto-load when
  working with cdpkit, chromey, tokio-tungstenite CDP envelopes, Browser.getVersion,
  Target.setAutoAttach, sessionId routing, Sender::send_raw, event_stream, Page.startScreencast,
  Page.screencastFrame, or Page.screencastFrameAck.
user-invocable: false
---

# Rust CDP Transport Reference

Evidence date: **2026-07-12**. Strict schema-v2 qualification selected exact published `cdpkit` **0.4.0** from accepted Linux and macOS reports emitted at gate revision `3d7c96ccf20862c47ab70ffbd7f724dceedfb4d2`, with configuration digest `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e` and the shared fixture digest recorded in the reports. The selection remains behind the replaceable adapter boundary; production lifecycle and capture implementation remain later work.

Full evidence and pinned sources: [`docs/research/rust-cdp-transport-2026-07.md`](../../../docs/research/rust-cdp-transport-2026-07.md).

## Qualified candidates

- `cdpkit` **0.4.0**, commit `15dd5e6d`, MIT OR Apache-2.0, MSRV 1.75.
- `chromey` **2.52.0**, commit `6eeca5e8`, MIT OR Apache-2.0, MSRV 1.70. Its Rust import name is `chromiumoxide`.
- Owned fallback: Tokio + `tokio-tungstenite` **0.30.0** (MSRV 1.85) with generated typed contracts from a pinned official CDP revision.
- Krometrail itself is Rust 1.85 and MIT.

## Non-negotiable boundary

Keep the selected client behind `krometrail-cdp::transport`. Core ports must not expose library types.

Krometrail owns:

- connection/reconnect policy and target restoration;
- browser/profile lifecycle;
- bounded frame handoff and capture gaps;
- compatibility/version evidence;
- cancellation and flush behavior.

A library may own one live WebSocket and request/event multiplexing. It must not silently reconnect or hide target reconstruction.

## `cdpkit` 0.4.0 API facts

```rust
use cdpkit::{page, target, CDP, Sender};
use futures::StreamExt;

let cdp = CDP::connect_ws(browser_ws_url).await?; // browser-level
let attached = target::methods::AttachToTarget::new(target_id)
    .send(&cdp)
    .await?; // flatten defaults true in 0.4.0
let session = cdp.owned_session(attached.session_id);

let mut frames = page::events::ScreencastFrame::subscribe(&session);
page::methods::StartScreencast::new().send(&session).await?;
while let Some(frame) = frames.next().await {
    page::methods::ScreencastFrameAck::new(frame.session_id)
        .send(&session)
        .await?;
    // Bounded handoff happens after prompt ack.
}

let value = session.send_raw("Page.getLayoutMetrics", serde_json::json!({})).await?;
let raw_params = session.event_stream::<serde_json::Value>("Page.lifecycleEvent");
```

Facts and pitfalls:

- `CDP` omits `sessionId`; `Session` and `OwnedSession` insert it.
- Session event subscriptions filter top-level `sessionId`.
- `event_stream::<Value>(name)` returns that event's `params`, not the full `{method, sessionId, params}` envelope; there is no wildcard raw stream.
- Each event subscription uses an unbounded channel. Never assume Krometrail's downstream bounded queue also bounds this channel.
- `CloseReason`, `is_closed`, and pending-call cleanup expose connection loss; there is no transparent reconnect.
- Generated `CDP_VERSION == "1.3"` does not identify the source protocol commit.

## `chromey` 2.52.0 API facts

```rust
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::page::{
    EventScreencastFrame, ScreencastFrameAckParams, StartScreencastParams,
};
use futures_util::StreamExt;

let (browser, mut handler) = Browser::connect(browser_endpoint).await?;
tokio::spawn(async move {
    while let Some(result) = handler.next().await {
        if result.is_err() { break; }
    }
});
let page = browser.new_page("about:blank").await?;
let mut frames = page.event_listener::<EventScreencastFrame>().await?;
page.start_screencast(StartScreencastParams::default()).await?;
while let Some(frame) = frames.next().await {
    page.ack_screencast(ScreencastFrameAckParams::new(frame.session_id)).await?;
}
```

Facts and pitfalls:

- `Browser::execute<T: Command>` sends browser-level typed commands; `Page::execute` sends session-scoped commands.
- Flat sessions are configured internally and routed through handler state.
- There is no first-class method-string raw command, but public `Command` and `Method` traits permit a local generic `Value` command.
- Named custom events are supported, but incoming messages pass through generated CDP decoding. GitHub issue #5 records a real unknown-enum drift failure.
- Initial connection retries do not replace Krometrail-owned runtime reconnection.
- Event listener channels are unbounded.

## Qualified real-browser result

The accepted `v2` reports and generated decision live under `docs/evidence/cdp-transport/v2/`. Both platform reports use exact `cdpkit` 0.4.0, the same immutable gate revision/configuration/fixture, canonical RSS sample/cadence/warmup measurements, observed lifecycle fields, and the same bound candidate-contract trace/hash/results object. Linux is digest `0d11c4c8168d8ef2e988b2f71400696dc8a9521add23ba645b9ea65a03e0b148`; macOS is digest `c206b1a04651421b8b88f42d75920800a75ee85ed83756f8792191a5e9b3b998` from hosted run `29202919716`. The measured screencast results are 3,601 frames in 60.013945058 seconds on Linux and 3,571 frames in 60.019783292 seconds on macOS, with 51 RSS samples on each. The generated decision retains platform-labelled gate and candidate-contract results for both Linux and macOS and selects `adopt_cdpkit`. The retained v1 reports and decision are historical only.

A candidate passes only when all hold:

1. Typed Browser/Page/Target/Runtime/Accessibility/Input operations succeed.
2. Two flat page sessions route commands and same-named events without crossing `sessionId`.
3. Browser- and session-scoped raw commands work; named raw event params survive additive fields.
4. Unknown event, additive-field, and unknown-enum fixtures do not terminate the connection or raw path.
5. At least 60 seconds and 1,000 screencast frames remain live while every frame is acknowledged before deliberately saturated bounded handoff.
6. Upstream subscriber memory remains bounded in practice; an unbounded library queue must not accumulate.
7. Disconnect resolves pending calls/subscriptions promptly; Krometrail can establish a new connection and rebuild sessions explicitly.
8. Ordering and disconnect cases are deterministic against a fake WebSocket without sleeps.

## Selection rules

- The selected candidate is exact `cdpkit` 0.4.0 because its source API best matched the adapter boundary and every unchanged v2 gate passed on both platforms.
- Keep it unchanged; needing a routing, decoder, lifecycle patch, or fork is failure.
- Try `chromey` only when its mature handler could address a demonstrated `cdpkit` lifecycle, ordering, or sustained-capture failure. Keep crawling/network policy out of core contracts.
- Choose the owned transport when either library loses unknown events before a raw boundary, obscures ack/backpressure, cannot route sessions reliably, or requires a fork.
- Never weaken a compatibility gate to select a dependency. Re-run the fallback decision only after a demonstrated cdpkit failure; do not pre-create or wire fallback production code.

## Owned fallback shape

Preserve raw envelopes as the source of truth:

```rust
struct IncomingEvent {
    method: String,
    session_id: Option<String>,
    params: serde_json::Value,
}
```

Correlate responses by request ID, route events by `sessionId`, and decode into generated typed contracts only after preserving the envelope. Generate the supported typed subset from a pinned official protocol revision; do not hand-copy a growing CDP surface.
