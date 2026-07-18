# Krometrail

**Browser memory for coding agents**

## Purpose

Krometrail gives local coding agents a visual memory of what a browser did over time.

Coding agents can operate browsers, inspect pages, and take screenshots, but their observations are usually isolated moments. This works when a defect persists in the final page state. It fails when the visible problem occurs only during a transition and disappears before the next screenshot.

Krometrail continuously records a controlled Chromium renderer session and lets an agent inspect selected intervals as compact visual evidence. The renderer may belong to Chrome, a compatible Chromium browser, or an explicitly debug-enabled Electron application. Krometrail exposes movement, jitter, flicker, transient layout changes, and incorrect intermediate states without requiring the model to consume video. When the local environment and agent host support it, the same retained evidence can also be presented as an optional video clip.

## Problem

Many browser defects are temporal:

- an element reverses direction during an animation;
- content flickers before settling;
- hydration briefly produces the wrong layout;
- a loading state appears and disappears too quickly;
- scrolling or focus jumps unexpectedly;
- two updates compete before the interface stabilizes;
- a canvas or game surface renders an incorrect intermediate frame.

A final screenshot can look correct even when the experience is visibly broken. Taking more screenshots manually is unreliable because the agent must anticipate the important moment and request each observation at the right time.

The missing capability is persistent visual history.

## Product Thesis

A coding agent becomes more capable when browser activity is represented as a queryable visual timeline rather than a sequence of manually requested snapshots.

Krometrail separates live operation from historical investigation:

1. Browser actions return the current visual and structured page state needed for ordinary agent operation.
2. The active Chrome-compatible renderer target is continuously recorded while the agent works.
3. Agent actions and browser activity mark the timeline.
4. The agent selects a relevant interval after observing a symptom.
5. Krometrail turns that interval into compact visual artifacts.
6. The agent reasons over the artifacts and requests source frames when more detail is needed.

The system preserves the evidence. The agent performs the diagnosis.

## Core Experience

Krometrail provides browser controls comparable to contemporary coding-agent browser tools. Significant actions return a post-action screenshot and structured page observation so an agent can see the current result without making a separate temporal query. Recording begins with the controlled browser session and continues without requiring the agent to predict when a defect will occur.

The live observation loop supports normal browser use. The temporal layer is an additional queryable history, not a replacement for current-state vision.

When an interaction looks suspicious, the agent can inspect the interval around its last action. A temporal query returns a compact package such as:

- a labeled storyboard of representative frames;
- a temporal difference map;
- selected source frames;
- capture timing and gap information;
- related action, navigation, console, and network markers.

The agent can expand a region or retrieve additional source frames without loading the entire recording into its context.

## Visual Evidence

Krometrail remains still-first: its default evidence is designed for models that reason over still images better than video. A qualified user-installed encoder can additionally produce a bounded MP4/H.264 presentation for models and hosts that accept video.

Its visual artifacts reorganize recorded frames into forms that make temporal behavior visible:

- storyboards show the sequence of meaningful states;
- difference maps show where and when pixels changed;
- region filmstrips expose short-lived local changes;
- motion-history views compress movement into a single image;
- optional video clips preserve a familiar time-based presentation without becoming authoritative evidence;
- source frames remain available behind every derived artifact.

Generated artifacts are lossy views of authoritative source frames. They carry their time range, source references, and transformation parameters. Inferred analysis is labeled separately from direct frame-derived transformations.

## Local-First Operation

Krometrail runs locally and controls Chrome-compatible renderer targets through the Chrome DevTools Protocol. It can explicitly attach to Electron renderer targets when the application exposes a local remote-debugging endpoint; Electron's Node main process is outside this browser-recording boundary. Captured frames and browser evidence remain on the user’s machine.

Recording is bounded by a user-configured disk budget rather than a short fixed history window. Krometrail evicts the oldest unpinned recording segments when the budget is reached. Important intervals can be pinned for continued investigation.

The primary environments are Linux and macOS.

## Reusable Temporal Vision

Temporal visual analysis is independent of browser capture.

A browser-agnostic Rust crate accepts timestamped image frames, markers, and optional regions of interest. It produces temporal visual artifacts and machine-readable provenance without depending on Chrome, CDP, DOM state, MCP, or Krometrail-specific types.

The crate lives inside the project while its interface develops, but its boundary supports later use in editor capture, game-engine tooling, computer-use systems, and other visual agent environments.

## Product Boundaries

Krometrail is:

- a browser flight recorder for local coding agents;
- a practical browser-control environment;
- a time-indexed visual evidence store;
- a still-first interface to browser motion and transient state, with optional derived video;
- an extensible foundation for optional browser-state evidence.

Krometrail is not:

- an automatic UI bug solver;
- a deterministic browser replay system;
- a production user-session observability service;
- a complete replacement for Chrome DevTools;
- a framework debugger as a prerequisite for useful capture;
- a model-training platform;
- a promise to diagnose every visual anomaly automatically.

DOM, accessibility, console, network, and framework evidence can enrich the visual timeline. None replaces visual recording as the product’s center.

## Success

Krometrail succeeds when a coding agent can:

1. launch and operate a browser with little setup;
2. receive current visual feedback during ordinary browser actions;
3. reproduce a defect whose final page state appears correct;
4. inspect the relevant visual interval after the interaction;
5. identify the transient behavior from compact visual artifacts appropriate to the agent host;
6. retrieve the underlying evidence needed to reason about the defect;
7. modify the application and verify that the visible temporal defect no longer occurs.

The decisive comparison is not against human video inspection. It is against an agent limited to current-state screenshots.

> Existing browser tools show an agent what the page is. Krometrail shows the agent what the page did.
