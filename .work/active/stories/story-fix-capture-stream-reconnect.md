---
id: story-fix-capture-stream-reconnect
kind: story
stage: done
tags: [bug, browser, visual]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Recover capture when its frame event stream closes

## Checkpoint

A generation-scoped `Page.screencastFrame` subscription failure must notify the browser-session supervisor and drive the same bounded reconnect that restores other transport event streams. Current-state control and retained capture must return on the replacement attachment generation.

## Root cause

The capture reader transitioned its private runtime to `Failed` when the frame event stream closed, but `CaptureObserver` had no failure signal for the session supervisor. Reconnect was therefore accidental: it occurred only when an independently owned target-event subscription also observed the physical connection failure. If that second signal was delayed or absent, retained capture stayed terminal even while other browser operations remained available.

## Acceptance evidence

- A scripted transport test closes only the generation-one `Page.screencastFrame` subscription while keeping the other event subscriptions open.
- The capture failure requests a generation-fenced supervisor reconnect.
- The replacement target reaches a newer attachment generation with active capture.
- Existing disconnect/reconnect and real-Chrome capture qualifications remain green.

## Implementation notes

- Added one capture-observer signal for a closed frame event stream. The production session translates it into the reducer's existing `ConnectionLost` input instead of introducing a second recovery owner.
- The signal carries the capture target's physical connection generation and enters through `ForConnectionGeneration`, so delayed readers from superseded connections are ignored.
- Extended the scripted CDP seam to close one named, session-scoped event stream without disconnecting the transport or its other subscribers.

## Verification

- Red regression: `cargo test -p krometrail-cdp closed_capture_frame_stream_reconnects_and_restores_capture_generation --locked -- --nocapture` timed out before implementation because no reconnect was requested.
- Focused regression: the same command passed after implementation and restored capture at attachment generation 2.
- Adapter suite: `cargo test -p krometrail-cdp --locked` passed.
- Real Chrome reconnect qualification: `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp opt_in_real_chrome_capture_fences_one_disconnect_and_resets_generation_identity --locked -- --nocapture` passed with 20 generation-one frames and 8 generation-two frames.
- Full suite: `cargo test --workspace --all-targets --locked` passed.

## Review

Bounded review found no correctness, compatibility, privacy, or documentation drift. The regression was tightened to wait for the public generation-two capture status rather than the slightly earlier scripted command notification, removing a scheduler-dependent assertion race.
