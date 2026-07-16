# Krometrail Architecture

## Architectural Intent

Krometrail separates browser-specific capture and control from temporal visual analysis.

The system has two central domains:

1. **Browser session recording** — Chrome-compatible renderer lifecycle, actions, targets, frames, events, timing, persistence, and temporal queries.
2. **Temporal visual analysis** — browser-agnostic transformation of timestamped frames into visual artifacts.

Browser infrastructure depends on domain contracts. The temporal visual crate does not depend on Krometrail.

## System Context

```text
┌────────────────────┐
│ Coding agent       │
│ Claude / Codex /   │
│ other MCP client   │
└─────────┬──────────┘
          │ MCP over stdio
          ▼
┌────────────────────────────────────────────────────┐
│ Krometrail                                         │
│                                                    │
│  MCP tools ──▶ application services                │
│                    │                               │
│          ┌─────────┼──────────┐                    │
│          ▼         ▼          ▼                    │
│     browser     recording   temporal queries       │
│     control     timeline         │                 │
│          │         │             ▼                 │
│          │         │      visual-analysis adapter  │
│          │         │             │                 │
│          ▼         ▼             ▼                 │
│       CDP adapter  recording store                 │
└──────────┬──────────────┬───────────────┬───────────┘
           │              │               │
           ▼              ▼               ▼
        Chrome      SQLite + segments  temporal visual
                                      crate
```

## Rust Workspace

```text
Cargo.toml
src/
  main.rs                    # Process entry and composition root

crates/
  krometrail-core/
    src/
      browser/               # Browser and target domain
      recording/             # Session, frame, interaction, and gap domain
      timeline/              # Range resolution and query orchestration
      capabilities/          # Capability registry
      ports/                 # Infrastructure interfaces
      error.rs

  krometrail-cdp/
    src/
      launcher/              # Chrome discovery, profiles, process lifecycle
      transport/             # WebSocket, commands, events, flat sessions
      targets/               # Target discovery and attachment
      capture/               # Screencast lifecycle and ingestion
      control/               # Navigation, snapshots, input, evaluation
      events/                # Console, network, lifecycle normalization

  krometrail-store/
    src/
      index/                 # SQLite metadata index
      segments/              # Append-only frame segment format
      retention/             # Budget accounting, pinning, and eviction
      recovery/              # Crash recovery and index reconciliation
      artifacts/             # Generated-artifact storage

  krometrail-mcp/
    src/
      config.rs              # Capability selection and injected service ports
      lib.rs                 # MCP adapter exports
      registry.rs            # Lifecycle and registry-derived tool routing
      resources.rs           # Canonical artifact/source-frame resource reads
      response.rs            # Structured, image, and resource-link responses
      schema.rs              # Generated input-schema projection
      server.rs              # MCP stdio protocol and server capabilities
      session.rs             # Single active browser-session ownership

  temporal-vision/           # Browser-agnostic temporal visual-analysis crate
    src/
      frame.rs               # Generic timestamped frame model
      sequence.rs            # Validated frame sequences
      measure/               # Direct visual-change measurements
      select/                # Representative-frame selection
      render/                # Storyboard and temporal artifact rendering
      provenance.rs          # Artifact manifests and source mapping
      error.rs

tests/
  fixtures/
    visual-defects/          # Reproducible browser defect applications
  integration/               # Real Chrome and storage tests
  agent/                     # Agent-level evaluation scenarios
```

The root binary is the composition root. It constructs infrastructure adapters and injects them into core services.

No infrastructure crate is imported by `krometrail-core`.

## Domain Model

### Identifier contracts

The foundation's implemented domain identifiers are opaque UUID-backed typed values:

```text
SessionId
TargetId
FrameId
InteractionId
MarkerId
SegmentId
ArtifactId
GapId
NavigationId
```

These IDs are declared together in `krometrail-core` so display, parsing, Serde,
ordering, and exhaustive round-trip coverage share one source of truth. The root
process adapter supplies collision-resistant UUID v4 values through the core
`IdSource` port; randomness remains outside the infrastructure-free domain.

Structured snapshot identifiers are implemented domain types shared by observation
and action requests:

```text
SnapshotGeneration  # non-zero generation for one target snapshot
SnapshotNodeId      # non-zero local node identity within a snapshot
NodeReference       # target, generation, and node identity
```

`PageSnapshot` owns one generation and validates preorder nodes, target scope, and
actionable references. The CDP control adapter owns the active per-target registry,
backing DOM-node bindings, document fingerprint, and attachment-generation fence;
its stale-reference boundary is described below.

The core timeline contains ordered observations:

```text
TimelineObservation
  session_id
  target_id
  session_time
  source_time?
  observed_time
  kind
  payload_reference
```

Observation kinds include:

- frame;
- interaction boundary;
- navigation;
- target lifecycle;
- visibility change;
- capture gap;
- console message;
- JavaScript exception;
- network lifecycle;
- marker.

Payloads live in the appropriate store. The timeline index contains the metadata required to locate and correlate them.

## Time Model

The authoritative ordering clock is `SessionTime`, a monotonic duration from the daemon’s session origin.

Every externally sourced observation also retains its native timestamp when one exists. Native timestamps are evidence, not the ordering authority.

```text
CapturedFrame
  source_time          # Chrome screencast timestamp
  observed_time        # daemon monotonic receipt time
  capture_ordinal      # Krometrail-observed order for this session and target
  session_time         # normalized timeline position
```

Agent interactions use the daemon monotonic clock for start, dispatch, completion, and observation points.

Clock conversion is explicit. Krometrail does not compare unrelated native clocks as if they share an epoch. Correlation uses normalized session time and exposes uncertainty introduced by buffering and encoding.

## Browser Connection

The CDP adapter treats Chrome pages and explicitly debug-enabled Electron renderer processes as the same renderer-target boundary. Electron's Node main process is a separate inspector surface and is not part of the browser adapter.

The CDP adapter owns:

- Chrome binary discovery;
- isolated profile paths;
- process launch and shutdown;
- attachment to an existing local Chrome-compatible endpoint, including an Electron renderer endpoint;
- browser WebSocket connection;
- flat target sessions;
- domain enablement;
- target discovery;
- reconnection.

The adapter exposes typed domain operations through ports defined by `krometrail-core`.

The production adapter uses exact cdpkit 0.4.0 behind the replaceable `krometrail-cdp::transport` boundary. `ProductionBrowserConnector` composes browser discovery and launch, compatibility probing, flat target sessions, supervised reconnect and target restoration, capture configuration, bounded frame handoff, recording, browser-event collection, and ownership-aware shutdown. The production path acknowledges each screencast frame before bounded handoff and records known loss as explicit capture-gap evidence.

The qualification spike remains a separate feature-gated, non-default test surface. Its cdpkit limitations remain binding: named-event parameters are not wildcard or full-envelope receive, the subscriber queue is unbounded and its depth is not inspectable, and cdpkit does not transparently reconnect or rebuild targets. Krometrail retains those responsibilities in the production supervisor; a routing, decoder, lifecycle patch, or fork would require a new transport decision.

A compatibility probe runs when connecting. It reports browser and protocol versions, identifies Electron renderer endpoints when detectable, and verifies the required domains before recording begins. Renderer support is decided from observed protocol capabilities rather than the host application's brand. The production `krometrail-cdp` adapter composes that probe with `ChromeLauncher`, a cdpkit transport seam, and a single-writer target/session supervisor. The supervisor subscribes to target events before enabling discovery and auto-attach, reconciles an initial snapshot, and rebuilds exact-key flat sessions after reconnect. Spike features remain non-default and are not root-wired.

## Target Lifecycle

A target supervisor tracks every recordable page target.

For each target it owns:

- exact browser target key and Krometrail `TargetId`;
- flat CDP session attachment and attachment generation;
- target lifecycle, including pre-suspension lifecycle during reconnect;
- current URL/title projection and visibility;
- the active structured-snapshot generation and reference registry.

Target creation and closure do not affect unrelated target streams. A target-level failure is reported without terminating the browser session unless the browser connection itself is lost. Target state is reduced by one serialized state machine; asynchronous transport and process tasks only submit inputs or execute emitted effects. Outbound session events use bounded subscriber channels with revision-gap recovery through `targets()`; cdpkit's private upstream queue is not represented as a measurable product metric.

## Capture Configuration Flow

Capture cadence is a session-owned part of the browser connection contract, not a process-wide configuration authority. The core browser port owns the typed `every_nth_frame` value and validates the inclusive 1..=60 boundary with a default of 1 on both launch and attach requests. The MCP lifecycle tools generate their schemas from those same request types, so humans and agents use one public contract.

At connection composition time, the CDP adapter copies the validated value into the immutable capture assembly used by every target stream and every reconnect generation in that session. Each `Page.startScreencast` command receives the value as `everyNthFrame`; no target, reconnect path, or status observer can select a replacement. A different value requires a new browser connection/session rather than an unrecorded mid-stream restart.

The requested stride is recorded alongside session and target capture status and in evaluation capture identities and claim traceability. It is interpreted as a deliberate sampling choice, while observed cadence, queue/persistence loss, visibility gaps, and other capture gaps remain independent evidence. The capture pipeline does not derive continuity or missing-frame claims from the stride or from ordinal arithmetic.

## Frame Ingestion

Frame ingestion is designed around CDP’s limited screencast window.

```text
Page.screencastFrame (receive)
        │
        ▼
immediately acknowledge CDP frame
        │
        ▼
decode envelope and timestamp
        │
        ▼
try bounded handoff of compressed frame ── enqueue fails ──▶ record explicit capture gap
        │
        ▼
segment writer
        │
        ├── append encoded bytes
        ├── append metadata
        └── update capture statistics
```

The event-reading task never performs image decoding, artifact generation, or synchronous disk I/O.

The ingestion queue is bounded. On receiving a frame, Krometrail immediately starts and completes the CDP acknowledgement before decoding and attempting bounded handoff. The event's `sessionId` integer is an opaque acknowledgement token in this boundary: it is echoed to `Page.screencastFrameAck` and is not persisted or compared. After acknowledgement, Krometrail assigns a per-target `CaptureOrdinal` that continues across attachment generations within the recording session. It deterministically orders observations but does not detect Chrome-side or transport-side loss; only explicit known loss and lifecycle events create gaps. Ack latency therefore measures only the interval from returned frame to acknowledgement completion, not frame receive wait or a wire-enqueue timestamp. When enqueue fails because the queue is saturated, Krometrail records an explicit capture gap after acknowledgement rather than stalling the browser connection or growing memory without limit. A deliberate `everyNthFrame` stride is applied by Chrome before this ingestion path and is not represented as a queue drop or inferred gap.

Compressed image bytes are stored without transcoding during ingestion.

## Capture Tasks

Each active visual stream has:

- a CDP event reader;
- a bounded frame-ingestion channel;
- a segment writer;
- capture statistics;
- cancellation and flush signals.

Disk writing can be shared across streams, but ordering is preserved independently per target.

CPU-intensive image work runs outside asynchronous I/O tasks on a bounded worker pool.

## Structured Snapshots and References

The core observation boundary owns validated structured snapshots and actionable
references. A `PageSnapshot` contains a target-scoped, non-zero generation and
preorder accessibility nodes. Each actionable node carries a `NodeReference`:

```text
NodeReference
  target_id
  generation
  node_id
```

The production CDP control adapter's per-target `SnapshotRegistry` retains the
active generation, attachment generation, document fingerprint, and backing DOM
node bindings. Before an action or reference-based region request it:

1. verifies the reference target and active generation;
2. verifies the attachment generation and current document fingerprint;
3. resolves the backing DOM node;
4. checks the requested visibility or actionability requirement;
5. obtains current geometry and dispatches the operation.

Navigation, document replacement, target closure, reconnect, and a fresh snapshot
invalidate the old generation. The adapter returns a structured stale-reference
error instead of guessing at a replacement node. Coordinate actions bypass
structured references and declare their coordinate space explicitly.

## Interaction Execution

An interaction is created before input is dispatched.

```text
create interaction
      │
      ├── mark start
      ├── resolve target
      ├── verify reference
      ├── mark dispatch
      ├── dispatch CDP input
      ├── await action-specific completion
      ├── capture live observation
      └── mark completion
```

The continuous recorder is independent of this path and captures frames throughout action execution.

Post-action screenshots use the first appropriate current-state observation after the action’s completion policy. They do not replace frames captured during the action.

Batch execution is sequential within a target. Different targets do not share an implicit action order.

## Recording Store

The store has two physical forms:

1. **SQLite index** for searchable metadata.
2. **Append-only segment files** for encoded frame payloads.

SQLite runs in write-ahead logging mode and contains:

- sessions;
- targets;
- frame indexes;
- segments;
- interactions;
- markers;
- capture gaps;
- browser events;
- pins;
- artifact manifests;
- usage accounting.

### Segment Format

A segment contains:

- format version;
- session and target identity;
- starting session time;
- ordered frame records;
- checksums;
- a sealed footer.

Each frame record contains a length-prefixed metadata header followed by the encoded image payload.

Only the current segment is mutable. Sealed segments are immutable.

Segment rotation is based on bounded duration and size. This makes retention deletion, pinning, recovery, and range reads operate on manageable units.

### Crash Recovery

An open segment is written in a recoverable record format.

On startup, the store:

1. locates unsealed segments;
2. scans complete frame records;
3. truncates incomplete trailing data;
4. seals recoverable segments;
5. reconciles SQLite frame indexes and usage accounting.

Metadata does not claim that a frame exists until its complete segment record is durable.

## Retention

The recording store is the authority for one global data-directory budget.

Budget accounting spans active and stopped sessions and includes:

- frame segments;
- indexes;
- browser-event payloads;
- generated artifacts.

Retention operates on sealed segments:

1. calculate total current usage;
2. identify the oldest unpinned segments across all sessions;
3. delete candidates in chronological order;
4. remove associated indexes and unprotected artifacts;
5. stop when usage is within budget.

Stopping a session leaves its segments retained and queryable. Pinning protects segments intersecting a requested range. Initial pinning is deliberately segment-granular.

When no unpinned data can satisfy the budget, the recorder enters a paused-budget state. It does not delete pinned evidence.

## Temporal Range Resolution

All temporal requests pass through one range resolver.

The resolver accepts explicit times, frame ranges, markers, navigations, and interaction-relative windows. It produces:

```text
ResolvedRange
  session_id
  target_id
  start_session_time
  end_session_time
  frame_ids
  interaction_ids
  gaps
  retention_warnings
```

Artifact generation consumes only resolved ranges. This prevents each artifact implementation from interpreting natural anchors differently.

## Temporal Visual Crate

The temporal visual crate is a standalone computation library.

It accepts generic frame input:

```text
Frame
  id
  timestamp
  width
  height
  pixel_format
  pixels

FrameSequence
  ordered frames
  optional markers
  optional region
  declared gaps
```

It returns:

```text
GeneratedArtifact
  artifact kind
  encoded image
  dimensions
  measurements
  provenance manifest
```

The crate:

- has no CDP dependency;
- has no MCP dependency;
- has no Krometrail domain dependency;
- performs no browser control;
- does not own session storage;
- does not infer logical UI elements;
- does not label defects as diagnosed facts.

Frame sources and artifact sinks are caller-provided. This permits both in-memory tests and Krometrail-backed range reads.

The crate is deterministic for identical frames, parameters, and algorithm version.

## Artifact Generation

A temporal query follows this path:

```text
MCP request
   │
   ▼
validate and resolve range
   │
   ▼
read encoded source frames
   │
   ▼
decode on bounded worker pool
   │
   ▼
adapt into temporal visual sequence
   │
   ▼
measure, select, and render
   │
   ▼
persist artifact and manifest
   │
   ▼
return summary + image/resource references
```

Artifact generation never blocks frame ingestion. Query concurrency and decoded-frame memory are bounded independently from recording.

A cache key derives from source-frame identities, artifact kind, parameters, and algorithm version. Identical artifact requests reuse retained outputs.

## Capability Registry

Capabilities are declared once as data:

```text
CapabilityDefinition
  id
  default_state
  dependencies
  recording_subsystems
  tools
  configuration_schema
```

MCP registration, configuration validation, status reporting, and subsystem startup derive from this registry.

A disabled capability contributes no MCP tools. Recorded browser-event evidence can remain enabled independently from event-inspection tools because capture and presentation are separate registry concerns.

Unavailable extension capabilities have contracts but no active implementation.

## MCP Boundary

The Rust MCP adapter uses the official Rust MCP SDK.

Tool handlers:

1. parse and validate external inputs;
2. invoke one application service;
3. translate domain results into MCP content;
4. map domain errors into stable external errors.

Tool handlers do not contain CDP commands, SQL, image processing, or retention logic.

Large binary outputs are persisted and returned as MCP resources or file references. A response can additionally include one context-sized image for immediate inspection.

Tool schemas derive from the same Rust contracts used by application services. Generated schemas are build artifacts, not hand-maintained duplicates. In particular, `start_browser` and `attach_browser` expose the same generated `every_nth_frame` field from the core launch and attach requests. The flat MCP adapter modules are `config.rs`, `registry.rs`, `resources.rs`, `response.rs`, `schema.rs`, `server.rs`, and `session.rs`; there are no parallel directory-based tool, schema, or response registries.

## Configuration

Process-wide configuration is validated once at process startup. Per-session capture choices are validated when the launch or attach request crosses the core/MCP boundary and then remain immutable for that connection.

Configuration sources follow explicit precedence:

1. command-line arguments;
2. environment variables;
3. user configuration file;
4. built-in defaults.

Configuration covers:

- Chrome binary and profile;
- launch or attach mode;
- initial URL;
- disk budget and data directory;
- screencast format, quality, and maximum dimensions;
- process-wide enabled capabilities;
- per-session capture stride supplied by the launch or attach request;
- ingestion and analysis concurrency;
- logging.

Invalid process configuration prevents startup and identifies the failing value. An invalid session request is rejected before browser connection/capture setup and identifies the failing field. There is no parallel CLI, environment-variable, or configuration-file authority for the session capture stride.

Qualification manifests record the requested stride in capture configuration identity, and evaluation claim/result traceability binds claims to that identity rather than treating reduced sampling as ordinary transport loss.

## Failure Isolation

Failures remain inside the narrowest responsible boundary:

- a tool validation failure does not reach the domain;
- a stale reference fails one action;
- a target closure ends one target stream;
- a frame-write failure records a gap and can pause that stream;
- an artifact failure does not interrupt capture;
- an SQLite failure stops persistence before accepting unsupported writes;
- an unrecoverable browser connection ends the session after flushing accepted data.

All long-running tasks participate in structured cancellation. Process shutdown waits for bounded flushing and then reports incomplete work.

## Observability

Krometrail emits local structured logs for:

- process and browser lifecycle;
- compatibility probe results;
- target discovery, attachment, visibility, suspension, closure, and local failure;
- reconnect attempts and session state transitions;
- bounded outbound subscriber lag;
- capture start and stop;
- frame cadence and gaps;
- queue saturation;
- segment rotation;
- retention decisions;
- temporal query timing;
- artifact cache behavior;
- errors and recovery.

Target logs use Krometrail target IDs and hashed or opaque browser keys. They do not include page text, titles, full URLs, query values, screenshot contents, credentials, event parameters, executable/profile paths, or sensitive network values by default. Adapter source errors remain debug-only and are mapped to stable core errors.

Session status exposes operational measurements needed to assess whether the recorder itself affected the observed application.

## Dependency Direction

```text
krometrail binary
  ├── krometrail-mcp
  ├── krometrail-cdp
  ├── krometrail-store
  ├── temporal-vision
  └── krometrail-core

krometrail-mcp   ──▶ krometrail-core
krometrail-cdp   ──▶ krometrail-core
krometrail-store ──▶ krometrail-core

krometrail-core  ──▶ no infrastructure crate
temporal-vision  ──▶ no Krometrail crate
```

The composition root provides an adapter around `temporal-vision` for the artifact-generation port defined by `krometrail-core`.

## Technology Decisions

- Rust 2024 edition provides the implementation language.
- Tokio provides asynchronous process, WebSocket, and task orchestration.
- The official Rust MCP SDK provides protocol transport and tool registration.
- Serde provides serialization.
- Generated JSON Schema defines external tool contracts.
- SQLite provides the searchable metadata index.
- Append-only files store compressed frame payloads.
- The `image` ecosystem provides initial decoding and rendering.
- Current final5 schema-v2 evidence selects exact cdpkit 0.4.0 behind a replaceable adapter. Its named-event-params and unbounded-subscriber limitations remain explicit, and the adapter must pass the required-domain compatibility probe at runtime.

OpenCV, FFmpeg, a browser extension, and framework-specific instrumentation are not architectural prerequisites.
