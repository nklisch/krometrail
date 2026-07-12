# Krometrail Specification

## Scope

Krometrail is a local browser-control and temporal-recording system for coding agents. A Rust daemon launches or attaches to Chrome, exposes browser operations through MCP, records visual and browser evidence, retains that evidence within a configurable disk budget, and generates temporal visual artifacts on demand.

This specification defines the system’s externally observable behavior. Visual artifact semantics are defined in [VISUAL-EVIDENCE.md](VISUAL-EVIDENCE.md). Internal component boundaries are defined in [ARCHITECTURE.md](ARCHITECTURE.md).

## Supported Environment

Krometrail supports:

- Linux;
- macOS;
- locally installed Chrome or Chromium-compatible browsers that expose the required Chrome DevTools Protocol domains;
- MCP clients using standard input and output transport.

Krometrail does not require the inspected application to install a package or modify its source code for visual capture and ordinary browser control.

## Browser Lifecycle

Krometrail can:

- launch an isolated browser profile;
- reopen a named Krometrail-managed profile;
- attach to an explicitly configured local CDP endpoint;
- open an initial URL;
- report browser, profile, target, and recording status;
- close a controlled browser or detach without closing it.

A launched profile has independent cookies, storage, permissions, and login state. Krometrail does not modify the user’s default browser profile unless the user explicitly chooses an attach workflow.

CDP endpoints bind to the local machine. Krometrail does not expose browser control over a public network interface by default.

## Sessions and Targets

A recording session begins when Krometrail connects to a browser and ends when recording is stopped or the connection becomes unrecoverable.

Each session has:

- a unique identifier;
- browser and protocol version information;
- profile identity;
- start and end times;
- configured disk budget;
- one or more page targets;
- a monotonic session clock;
- capture statistics;
- capability configuration.

Krometrail discovers page targets created within the controlled browser. Each target has an independent visual stream and timeline identity.

The session records target creation, closure, navigation, visibility changes, and periods in which Chrome does not provide visual frames. Background or hidden tabs are not represented as continuously visible when Chrome pauses their screencast.

## Continuous Visual Capture

Visual capture starts automatically for recordable page targets.

Krometrail requests every available screencast frame by default rather than deliberately reducing the stream to a low fixed sampling rate. Actual capture cadence depends on Chrome, page visibility, rendering activity, resolution, encoding cost, and local system load.

Each captured frame records:

- frame identifier;
- target identifier;
- Chrome screencast sequence identifier;
- Chrome-provided timestamp when available;
- daemon receive time on the session clock;
- encoded image format;
- image dimensions;
- viewport dimensions;
- device scale information;
- storage segment and byte offset;
- capture warnings associated with the frame.

Krometrail acknowledges CDP screencast frames promptly after accepting them into a bounded ingestion queue. Disk writes and image analysis do not block CDP acknowledgement.

If Krometrail cannot accept or persist a frame, it records a capture-gap event. It does not silently imply that adjacent stored frames form a complete sequence.

The source frame stream is authoritative. Generated artifacts are derived views.

## Current-State Observation

Temporal history does not replace ordinary browser vision.

Every standalone state-changing browser action returns a live observation containing:

- action status;
- action start and completion times;
- current URL and target;
- a post-action screenshot;
- a structured page snapshot or a reference to one;
- relevant navigation or dialog state;
- an identifier that anchors the action in the temporal timeline.

State-changing actions include navigation, history movement, reload, click, text input, key input, selection, hover, drag, scrolling, dialog handling, and file upload.

Krometrail chooses an action-appropriate observation point after dispatching the action. It does not wait for complete network idleness unless the requested action requires it. The temporal recorder continues capturing intermediate states while the action completes.

Read-only inspection operations do not generate additional screenshots unless requested.

A batch of actions returns per-step status and timeline anchors. It returns a final live observation and can include per-step screenshots when requested.

## Structured Page Snapshots

Krometrail exposes a compact structured representation of the current page for agent navigation and element targeting.

The snapshot includes relevant accessible content, roles, names, values, states, and actionable nodes. It may include DOM-derived information where necessary to resolve interaction geometry.

Element references are scoped to the snapshot generation that produced them. They are not stable identities across navigation, rendering, or DOM replacement.

Before executing an action against a reference, Krometrail verifies that the backing node remains valid and actionable. It fails with a specific stale-reference error when it cannot safely resolve the target. Errors instruct the agent to refresh the snapshot.

Canvas, WebGL, video, and other DOM-opaque surfaces remain visible through screenshots and temporal capture even when structured targeting is unavailable. Coordinate-based interaction is available as a fallback capability.

## Browser-Control Surface

The control capability provides operations for:

### Browser and pages

- start, attach, stop, and status;
- list pages;
- create, select, and close a page;
- navigate;
- reload;
- move backward or forward in page history.

### Inspection

- obtain the current structured snapshot;
- take a screenshot of the viewport, full page, element, or region;
- evaluate JavaScript in the page context;
- inspect current URL, title, viewport, and navigation state.

### Interaction

- click;
- fill or type;
- press keys and key combinations;
- choose a select option;
- hover;
- drag;
- scroll by an offset or to a target;
- upload a file;
- accept or dismiss a browser dialog.

### Waiting

- wait for elapsed time;
- wait for text or an element state;
- wait for navigation;
- wait for a page condition;
- wait for network quiet when explicitly requested.

### Batching

Related actions can be submitted as an ordered batch. Every step receives its own status, timing, and timeline anchor. Execution stops on the first failed step unless the request explicitly permits continuation.

The exact MCP schemas derive from a single capability registry and shared domain contracts.

## Action Timeline

Every agent browser action creates an interaction record containing:

- interaction identifier;
- target;
- action type;
- sanitized action parameters;
- start time;
- dispatch time;
- completion time;
- live-observation time;
- outcome;
- related navigation;
- capture-gap warnings;
- parent batch identifier when applicable.

Temporal queries can use interaction identifiers without requiring the agent to calculate timestamps.

## Browser Events

Krometrail records lightweight browser evidence available through CDP:

- console messages;
- uncaught JavaScript exceptions;
- request and response lifecycle metadata;
- failed requests;
- navigation and lifecycle events;
- target and visibility changes;
- browser dialogs.

Sensitive request headers, cookies, authentication values, and request or response bodies are not persisted by default.

Browser-event recording is independent of whether event-inspection MCP tools are exposed. This allows capability experiments without changing the recorded session format.

## Capabilities

The MCP tool surface and active recording subsystems derive from a capability registry.

Defined capabilities are:

| Capability | Default | Responsibility |
|---|---:|---|
| `control` | enabled | Browser lifecycle, structured snapshots, current screenshots, interaction, and waiting |
| `temporal-vision` | enabled | Continuous visual capture, temporal ranges, visual artifacts, and source-frame retrieval |
| `browser-events` | enabled | Console, exception, network metadata, navigation, and lifecycle evidence |
| `page-state` | unavailable | Rich DOM, layout, storage, and application-defined state evidence |
| `framework-state` | unavailable | Framework component, render, and state evidence |

Unavailable capabilities are extension points, not commitments required for the core product.

Tools belonging to a disabled capability are not registered with MCP. The registry is the single source of truth for capability names, dependencies, configuration, and tool membership.

## Disk Budget and Retention

The Krometrail data directory uses one configurable global disk budget across active sessions, retained sessions, indexes, browser events, and generated artifacts. The default budget is 10 GB.

Recorded data is stored in time-based immutable segments. Session metadata and artifact indexes are stored separately from frame payloads. Stopping a session leaves its retained ranges queryable under the global budget.

When total stored data exceeds the budget, Krometrail removes the oldest unpinned segments across all sessions until usage returns below the limit.

A time range can be pinned. Pinning protects every storage segment required to reconstruct that range. If pinned data consumes the entire budget, recording pauses before deleting protected evidence and reports the condition clearly.

The status surface reports:

- configured budget;
- current usage;
- pinned usage;
- oldest retained time;
- newest retained time;
- capture cadence;
- recorded and dropped frames;
- whether eviction or recording is blocked.

Stopping a session flushes accepted frames and metadata before reporting completion.

## Temporal Ranges

A temporal range can be specified by:

- explicit session-relative time;
- explicit timestamps;
- interaction identifier;
- a window before and after an interaction;
- the most recent interaction;
- navigation or marker identifier;
- a source-frame range.

Natural anchors resolve to an explicit target and time range before artifact generation. The resolved range is returned with the response.

Queries fail clearly when part or all of the requested range has been evicted, was never captured, belongs to a different target, or contains known capture gaps.

## Temporal Queries

The temporal-vision capability supports:

- inspect a range;
- inspect a window around an interaction;
- generate a temporal storyboard;
- generate a temporal difference map;
- generate supported region-focused artifacts;
- list source frames and capture gaps;
- retrieve selected source frames;
- pin or unpin a range;
- report visual-change measurements;
- combine related outputs into a temporal debug bundle.

A temporal debug bundle contains a concise text summary, artifact references, source-frame references, provenance, resolved timing, and capture-quality warnings.

Large images and raw frame collections are returned by file or MCP resource reference. The response may include a context-sized primary image for immediate model inspection.

## Regions of Interest

A temporal query can focus on:

- fixed viewport coordinates;
- a region selected from a source frame;
- the current geometry of a structured page reference;
- a caller-provided mask.

A fixed visual region does not imply logical element tracking. If the page scrolls, resizes, or replaces content, the region remains tied to its declared coordinate space unless a tracking method explicitly states otherwise.

Every region-focused artifact includes enough surrounding context to locate the region within the page.

## Artifact Provenance

Every generated visual artifact records:

- artifact identifier and type;
- session and target;
- resolved time range;
- ordered source-frame identifiers;
- omitted-frame count;
- known capture gaps;
- crop or mask;
- dimensions and scale;
- transformation parameters;
- algorithm version;
- whether the output is source-derived or inferred.

An agent can retrieve the source frames behind any artifact while they remain retained.

## Errors and Degraded Operation

Boundary failures return structured errors containing:

- stable error code;
- plain-language explanation;
- affected session, target, interaction, or range;
- whether retry is safe;
- a concrete recovery action when one exists.

Krometrail degrades explicitly:

- stale page references require a new snapshot;
- a disconnected browser enters reconnecting state;
- unrecoverable disconnection closes the session after flushing accepted data;
- a hidden target records a visibility gap;
- a saturated ingestion queue records dropped frames;
- an exhausted disk budget pauses capture when protected data prevents eviction;
- an unsupported CDP command reports the detected browser and protocol versions.

## Local Data and Telemetry

Captured browser data remains local unless the user or connected agent explicitly reads an artifact through MCP.

Krometrail does not send usage telemetry, session contents, update checks, or derived artifacts to an external service by default.

Session data can be deleted by session identifier. Deletion removes source frames, browser events, generated artifacts, and indexes associated with the session.

## Extension Boundaries

The core contracts permit additional evidence adapters to contribute timestamped observations to the session timeline.

An adapter cannot redefine source-frame timing, browser interaction identity, retention behavior, or visual artifact provenance. Browser-specific evidence depends on the session domain; the temporal visual crate remains independent of browser and framework types.

## Exclusions

The system does not guarantee:

- every browser-rendered frame is captured;
- presentation-time-perfect timestamps;
- uninterrupted capture from hidden or throttled tabs;
- permanent element references;
- automatic causal attribution;
- automatic visual-defect diagnosis;
- deterministic replay;
- logical element tracking across recreation;
- framework-state availability;
- support for non-Chromium browser engines.
