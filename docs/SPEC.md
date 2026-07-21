# Krometrail Specification

## Scope

Krometrail is a local browser-control and temporal-recording system for coding agents. A Rust daemon launches Chrome or attaches to a compatible Chromium renderer endpoint, exposes browser operations through MCP, records visual and browser evidence, retains that evidence within a configurable disk budget, and generates temporal visual artifacts on demand.

This specification defines the system’s externally observable behavior. Visual artifact semantics are defined in [VISUAL-EVIDENCE.md](VISUAL-EVIDENCE.md). Internal component boundaries are defined in [ARCHITECTURE.md](ARCHITECTURE.md).

## Supported Environment

Krometrail supports:

- Linux;
- macOS;
- locally installed Chrome or Chromium-compatible browsers that expose the required Chrome DevTools Protocol domains;
- Electron renderer processes exposed through an explicitly enabled local remote-debugging endpoint;
- MCP clients using standard input and output transport.

Krometrail does not require the inspected application to install a package or modify its source code for visual capture and ordinary browser control. Electron attachment requires the application to opt into a local Chromium remote-debugging endpoint; Krometrail does not instrument or control the Electron Node main process.

Video export is optional. Krometrail does not download, bundle, or redistribute FFmpeg. At MCP startup it qualifies a user-installed `ffmpeg` executable and an available MP4/H.264 encoding path. When qualification fails, the temporal-video capability and its tools are omitted while browser control, recording, and still-image artifacts continue normally.

## Browser Lifecycle

Krometrail can:

- launch an isolated browser profile;
- reopen a named Krometrail-managed profile;
- attach to an explicitly configured local CDP endpoint;
- open an initial URL;
- report browser, profile, target, and recording status;
- close a controlled browser or detach without closing it.

By default, Krometrail launches an isolated reusable managed profile. A launched profile has independent cookies, storage, permissions, and login state. Attaching to an existing debug-enabled Chrome, selecting another named profile, or requesting a temporary profile is explicit. Krometrail does not modify the user’s default browser profile unless the user explicitly chooses an attach workflow.

CDP endpoints bind to the local machine. Krometrail does not expose browser control over a public network interface by default.

## Sessions and Targets

A recording session begins when Krometrail connects to a browser or Electron renderer endpoint and ends when recording is stopped or the connection becomes unrecoverable.

Each session has:

- a unique identifier;
- browser and protocol version information;
- profile identity;
- start and end times;
- configured disk budget;
- one or more page targets;
- a monotonic session clock;
- capture statistics;
- immutable capture configuration, including the requested relative frame stride;
- an immutable managed-launch focus policy;
- capability configuration.

Krometrail discovers page targets created within the controlled browser. Each target has an independent visual stream and timeline identity.

The session records target creation, closure, navigation, visibility changes, and periods in which Chrome does not provide visual frames. Renderer-created popup targets are adopted when their navigation becomes recordable and retain their opener relationship. Background or hidden tabs are not represented as continuously visible when Chrome pauses their screencast.

## Continuous Visual Capture

Visual capture starts automatically for recordable page targets.

The launch and attach requests accept one optional `every_nth_frame` relative stride. It is an integer from 1 through 60 and defaults to 1. Krometrail passes the value to CDP `Page.startScreencast.everyNthFrame`. This is a best-effort sampling request, not an exact FPS contract: actual capture cadence still depends on Chrome, page visibility, rendering activity, resolution, encoding cost, local system load, and whether frames reach and pass the bounded ingestion path.

The requested stride is immutable for the browser connection and recording session. Krometrail does not change it by stopping and restarting capture mid-session; a caller that needs a different stride starts a new session. A stride greater than 1 intentionally reduces capture probability and is reported as capture configuration provenance. It does not turn ordinary queue drops, persistence failures, visibility pauses, or other known losses into deliberate stride events, and those losses remain separate capture gaps.

Each captured frame records:

- frame identifier;
- target identifier;
- a Krometrail-owned per-target capture ordinal for deterministic ordering;
- Chrome-provided timestamp when available;
- daemon receive time on the session clock;
- encoded image format;
- image dimensions;
- viewport dimensions;
- device scale information;
- storage segment and byte offset;
- capture warnings associated with the frame.

Krometrail acknowledges each received CDP screencast frame immediately, before attempting bounded handoff. The CDP frame event's `sessionId` integer is retained only long enough to acknowledge that event; Krometrail does not treat it as frame ordering or continuity evidence. Ack latency covers only the interval from frame receipt to acknowledgement completion; disk writes and image analysis do not block CDP acknowledgement. The production acknowledgement deadline defaults to one second. Acknowledgement is one-shot: failure beyond the configured deadline is terminal for that capture stream, creates one explicit gap, and never retries an ambiguous token.

After acknowledgement, Krometrail assigns a non-zero capture ordinal that increases for every acknowledged frame event observed for that session and target, including events later rejected or dropped. The ordinal supplies deterministic local order, including when monotonic clock readings are equal. It does not prove that Chrome emitted every rendered frame or that every Chrome event reached Krometrail, and ordinal arithmetic does not create inferred gaps.

If bounded handoff fails because the ingestion queue cannot accept the frame, Krometrail records an explicit capture-gap event after acknowledgement. It does not silently imply that adjacent stored frames form a complete sequence. A later rejection or persistence failure is likewise represented as a capture gap.

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

Krometrail chooses an action-appropriate observation point after dispatching the action. Automatic post-action screenshots wait for two renderer animation-frame callbacks under a bounded 250 millisecond cap; a missing signal degrades evidence but does not erase a proven dispatch or mutation. It does not wait for complete network idleness unless the requested action requires it. The temporal recorder continues capturing intermediate states while the action completes.

Action dispatch or mutation, current-state observation, and retained temporal capture are independent outcomes. A failed retained-capture stream reports its first bounded failure stage and sanitized cause on later operations while current-state control remains available. Persistence causes identify a closed operation, category, and writer recoverability without paths, raw operating-system messages, frame data, or page content. Observation failure after a proven action produces a degraded non-error response and must not imply that replay is safe.

Read-only inspection operations do not generate additional screenshots unless requested.

MCP responses use one detail progression: `concise`, `expanded`, and `full`. Omitting response preferences selects
`concise`. Projection changes presentation only: the action outcome, interaction anchor, warnings, retained evidence,
and canonical resource identities remain authoritative and available. Concise action responses contain the current
page identity, navigation/focus changes, and a flattened generation-scoped target index rather than a pruned page
tree. Expanded responses add bounded semantic and page context. Full responses expose the complete acquired
structures. Image transport remains independent from structured detail. Explicit visual operations (`take_screenshot`,
`observe_live`, `temporal_debug_bundle`, `fetch_source_frames`, `generate_artifacts`, and
`generate_region_filmstrip`) include one bounded requested or primary image when `inline_images` is omitted. Stale-prone
post-action observations for `scroll`, `set_viewport`, and `activate_page` include one bounded viewport image by
default; other routine operations remain pixel-light. `inline_images: false` suppresses pixels and `inline_images: true`
requests them where supported. Failed and degraded responses always include privacy-bounded diagnostics; callers cannot suppress the only
actionable failure evidence. Status requests use the same concise-to-full direction.

Concise interaction results retain the interaction anchor and post-action observation; expanded and full results add
the sanitized interaction record echo for callers that need parameter and timing provenance. A pre-dispatch failure
may retain an interaction context anchor with its known identity, target, and operation, but omits timing fields when
no dispatch interval was observed; it never uses zero timestamps as a placeholder.

A batch of actions returns per-step status and timeline anchors. It returns a final live observation and includes per-step screenshot fields only when screenshots were requested.

## Structured Page Snapshots

Krometrail exposes a compact structured representation of the current page for agent navigation and element targeting.

The snapshot includes relevant accessible content, roles, names, values, states, and actionable nodes. It may include DOM-derived information where necessary to resolve interaction geometry. A caller may select the main document or a qualified same-origin, same-process frame. Frame-scoped snapshots are the structured inspection path for non-actionable frame content; frame-scoped semantic queries remain actionable-reference discovery.

Concise snapshot presentation ranks focused targets, editable targets, other non-link targets, and links, using
canonical preorder to break ties. It emits a flattened bounded target index with each exact generation-scoped
reference and the role, name, value, and salient non-default states required for interaction; structural ancestors
and false boolean state defaults are not repeated.
Structural web-area and document roots do not become interaction targets from generic focusable or clickable signals.
Expanded presentation adds bounded semantic structure and context. Full presentation returns the complete acquired
snapshot. Every bounded projection reports exact presentation omissions. When an automatic post-action observation
reuses the current snapshot generation, concise and expanded responses emit an unchanged-generation marker containing
the generation, `unchanged: true`, target count, and omission counts; explicit inspection remains the drill-down
authority.

Automatic live observations also include a small bounded `semantic_outcomes` list prioritizing current alerts,
dialogs, status messages, and named text. It describes current post-action state and does not claim a pre/post change.

Element references are scoped to the target attachment and document generation that produced them.
A later snapshot of the same attached document preserves a reference while its backing DOM node
remains present. References are not stable across navigation, document replacement, reconnect, or
backing-node replacement.

Before executing an action against a reference, Krometrail verifies that the backing node remains valid and actionable. It fails with a specific stale-reference error when it cannot safely resolve the target. Errors instruct the agent to refresh the snapshot.

Snapshot references are the primary target form. Explicit CSS selectors remain a debugging escape hatch with weaker validation guarantees. Canvas, WebGL, video, and other DOM-opaque surfaces remain visible through screenshots and temporal capture even when structured targeting is unavailable; declared coordinate-space interaction is the final fallback.

Callers can also describe a semantic locator by accessible role and name, label text, visible text, or test
identifier, optionally scoped to a descendant and a qualified same-origin/same-process frame (including a
same-process `about:srcdoc` or `about:blank` frame whose opaque origin is inherited from its parent).
Fresh opaque child documents such as `data:` frames are not same-origin-qualified, even when their
parent is also opaque. A role query may qualify an unnamed
control by text rendered within its nearest matching ancestor container; this bounded relationship never
falls back to spatial proximity or unrelated page text. Krometrail resolves the locator through
the active document snapshot registry and returns or acts through an exact generation-scoped reference. A no-match,
ambiguous, or truncated result is an explicit successful query outcome, but it contains no actionable
reference and never authorizes mutation. A no-match result for a query that used an exact text matcher
additionally reports how many nodes the same query would have matched with every exact matcher relaxed to
`contains`, so a decorated accessible name such as `"Cargo.toml, (File)"` produces one informed follow-up
rather than a guess-and-retry loop. That count is scanned over the same bounded snapshot nodes, is capped at
a declared candidate limit, and reports saturation at the cap. It is omitted when the query used no exact
matcher, when the relaxation would still match nothing, or when the query matched. Semantic matching never silently selects one of several or potentially
unreported nodes; callers narrow the query until it returns one unique reference before acting. Plain
role/name queries acquire only the selected document's accessibility tree. Container-text, label,
rendered-text, and test-id queries additionally acquire DOM semantics. Completeness limits apply to
the selected acquisition, not unrelated documents, and fail with bounded narrowing guidance.

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
- inspect privacy-bounded page assets and frame structure;
- read or write the selected page clipboard only through an explicit tool request;
- observe popup and download lifecycle metadata and retrieve completed local download resources within the
  managed session boundary.

Page-asset acquisition remains bounded and complete before presentation. Concise responses include a small row
sample, deterministic counts by asset kind, and separate source-versus-presentation omission counts. Expanded
responses include a broader bounded sample; full returns every acquired row.

#### Frame identity

Frame listings must let an agent tell one frame from another. Each listed frame reports its author-assigned
`name` from the browser's frame tree, bounded in length and free of control characters. A frame whose document
shares the main document's origin — the main document itself and same-origin, same-process frames — also
reports its real URL path.

That same-origin path is the only place Krometrail reports an unhashed URL path. It is bounded, carries no
query string, fragment, or credentials, and is defensible because the agent can already read that frame's full
content through a frame-scoped snapshot and its full URL through page evaluation in the same session over the
same tool surface; hashing it cost targeting and protected nothing. Cross-origin, out-of-process, and
indeterminate frames report no path, and origin-preserved path hashing everywhere else — browser network
events above all — is unchanged.

#### Open dialogs

An open modal JavaScript dialog blocks the renderer, so it is reported state rather than an inference drawn
separately at each surface. Page status reports, per page, whether a dialog is open and of what type, or
`unknown` when dialog observation is not installed for that page. Because a dialog blocks the page, browser
status names the affected pages at every detail level rather than only where full page rows appear. A renderer operation that fails while a
dialog is known to be open reports a distinct dialog-open error code whose recovery is to handle the dialog,
not to retry or inspect browser compatibility. `handle_dialog` against a page with no open dialog reports the
stable not-found boundary with a structured code rather than a generic rejection.

### Viewport emulation

- apply explicit width, height, device scale, mobile-layout, and touch metrics to one selected page;
- select a named responsive-CSS or mobile-device preset that materializes into those same explicit metrics;
- clear that target-scoped override and report the independently observed effective CSS visual viewport.

Overrides survive ordinary navigation and same-target reattachment, are restored before capture
resumes, and never affect another target. A clear removes both device metrics and touch emulation. For
responsive desktop and custom desktop overrides, Krometrail acknowledges exact declared layout geometry and
reports visual content separately; the visual content area can be narrower when Chrome reserves scrollbar space.
For mobile overrides, Krometrail acknowledges the exact visual viewport because mobile emulation, page scale,
and viewport metadata intentionally control that surface. The result identifies the selected intent and provides
guidance when observed layout and visual content materially differ, including the common missing-viewport-metadata
case. Presets do not imply user-agent emulation; custom metrics and clear retain their stable meanings.

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

#### File upload path authority

File upload carries the local operator's own filesystem authority. Krometrail
canonicalizes each operator-supplied path, resolving symlinks, requires the
result to be a regular readable file, and passes that canonical path to the
browser. There is no configured upload root and no containment check.

This is deliberate. Krometrail is a local single-user tool whose caller already
holds the operator's filesystem access, and upload paths are always supplied
explicitly by the caller — a web page cannot choose or influence which path is
uploaded. A containment root would remove the legitimate ability to upload from
arbitrary local paths while adding no boundary the caller could not already
cross directly.

Krometrail therefore claims no containment guarantee for upload. Callers are
responsible for the paths they supply.

### Waiting

- wait for elapsed time;
- wait for text or an element state;
- wait for navigation;
- wait for a page condition;
- wait for network quiet when explicitly requested.

### Batching

Related actions can be submitted as an ordered batch. Every step receives its own status, timing, and timeline anchor. Execution stops on the first failed step unless the request explicitly permits continuation.

A failed batch reports the same evidence a degraded batch reports: every step result the batch produced, and a top-level error naming the failing step index, its operation, its stable error code, and its cause. There is no separate failure shape that drops that evidence, and a failed operation's summary text states its cause rather than only naming the tool.

The MCP surface provides composable standalone tools plus batching. Both derive actions and schemas from the same capability registry and shared domain contracts.

Page-scoped requests select the current page when `target` is omitted. Click defaults to the left
button, no modifiers, one click, and no navigation wait; fill defaults to replace mode and no
navigation wait; key input defaults to no focus locator and no navigation wait. Explicit request
values have the meanings declared by the current generated schema.

Managed launches accept a `focus` policy of `foreground` or `preserve`; omission defaults to
`foreground`. The policy governs Krometrail-owned CDP foreground commands
after launch. It does not promise that the operating system will keep the initial Chrome process
launch in the background. Under `foreground`, pointer, drag, and scroll operations foreground a
hidden page through the managed CDP target authority before resolving or dispatching input. Under
`preserve`, Krometrail sends neither `Target.activateTarget` nor `Page.bringToFront`: current visible
page actions continue normally, while hidden-page pointer work fails as `target_hidden` before any
pointer event is dispatched. `activate_page` is the deliberate one-shot exception: it foregrounds
the selected or explicitly named target, waits boundedly for visible document state, and returns
live evidence without changing the immutable session policy. A failed activation remains
`target_hidden` and dispatches no pointer input.

Creating or selecting a page always updates Krometrail's logical selected target. In `foreground`
mode it also activates the Chrome target. In `preserve` mode newly created targets are explicitly
created in the background and neither creation nor selection switches Chrome's visible tab.
Activation does not change Krometrail's logical selected target; omit its target only to activate
the current selection.
Attachment retains foreground behavior because the attached browser remains externally owned.

Browser discovery probes explicit, environment, and platform-default installations with a bounded
cold-start budget while keeping PATH probes short. Candidate diagnostics contain only source class,
ordinal, outcome, and elapsed time; executable paths and version output are not logged.

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

### Text redaction is best-effort

Console text, exception messages, and stack function names pass through a
bounded sanitizer that replaces recognizable secrets, absolute filesystem paths,
and URLs with fixed placeholders, and truncates to a byte limit.

**This is best-effort defence in depth, not a guarantee, and it is not a
security boundary.** The sanitizer is a heuristic over arbitrary page-authored
text. It recognizes common shapes — sensitive key names, quoted values, URLs,
POSIX and Windows paths — and will miss unusual encodings, unfamiliar key names,
and secrets that do not look like secrets. Krometrail does not claim, and callers
must not assume, that recorded browser-event text is free of sensitive material.

What Krometrail does guarantee is narrower and structural: the categories listed
above (headers, cookies, authentication values, request and response bodies) are
not persisted at all, and event text is bounded in size. Those are properties of
what is collected, not of what a sanitizer managed to detect.

Callers who require that no page-authored text is retained should disable
browser-event recording rather than rely on redaction.

A missed redaction is a bug worth fixing when observed, and the corpus in
`crates/krometrail-core/src/browser/privacy.rs` is the contract — extend it when
a new shape appears. It is not a release gate.

Browser-event recording is independent of whether event-inspection MCP tools are exposed. This allows capability experiments without changing the recorded session format.

## Capabilities

The MCP tool surface and active recording subsystems derive from a capability registry.

Defined capabilities are:

| Capability | Default | Responsibility |
|---|---:|---|
| `control` | enabled | Browser lifecycle, structured snapshots, current screenshots, interaction, and waiting |
| `temporal-vision` | enabled | Continuous visual capture, temporal ranges, visual artifacts, and source-frame retrieval |
| `temporal-video` | conditional | Bounded MP4/H.264 clips derived from retained source frames through a qualified user-installed encoder |
| `browser-events` | enabled | Console, exception, network metadata, navigation, and lifecycle evidence |
| `page-state` | unavailable | Rich DOM, layout, storage, and application-defined state evidence |
| `framework-state` | unavailable | Framework component, render, and state evidence |

Unavailable capabilities are extension points, not commitments required for the core product.

Tools belonging to a disabled or startup-unavailable capability are not registered with MCP. The registry is the single source of truth for capability names, dependencies, runtime availability, configuration, and tool membership. `temporal-video` depends on `temporal-vision`; its absence is discoverable through local startup diagnostics and shipped agent guidance rather than a nonfunctional placeholder tool. Installing or changing FFmpeg requires an MCP restart before the advertised capability can change.

## Disk Budget and Retention

The Krometrail data directory uses one configurable global disk budget across active sessions, retained sessions, indexes, browser events, and generated artifacts. The default budget is 10 GB, and it is a total shared across every concurrently running instance rather than a per-process allowance.

Recorded data is stored in time-based immutable segments. Session metadata and artifact indexes are stored separately from frame payloads. Stopping a session leaves its retained ranges queryable under the global budget.

Segment publication treats writes, flushes, file sync, and rename as terminal when their resulting offsets or namespace state are ambiguous. A directory-sync failure after a completed sealed-file rename rejects the triggering append but leaves the writer usable for a later session: the sealed path is already authoritative and the next append creates a distinct segment. Capture status and stop results preserve that distinction. A writer-usable cause directs the agent to start a new browser session; a writer-terminal cause requires restarting the Krometrail MCP process first.

Krometrail supports one current metadata-index format. It initializes an empty store directly to
that format and opens exact current-format data without rewriting it. An unversioned non-empty,
older, or newer index is classified before schema mutation. Krometrail then clears the disposable
recording cache—the index and sidecars, retained segments, generated artifacts, and deletion
staging—initializes the current format, and continues startup. It does not migrate historical
formats. Configuration, managed browser profiles, diagnostics, and other data-root members are not
recording cache and remain untouched.

### Instance isolation and the shared budget

Each Krometrail process owns one instance root under `instances/<uuid>/` in the data directory and holds an advisory lock on it for its lifetime. A process never reads or mutates another instance's storage, so a second process cannot disturb a running one's capture. Evidence is therefore scoped to the instance that recorded it: after the Krometrail process restarts, evidence recorded by a previous process is no longer queryable.

Reclaiming an abandoned root deletes data, so it runs only where exclusive ownership can be *proved*. Linux and macOS prove it with `flock`. On any other host — including the best-effort Windows build — a root a process does not hold is treated as live: abandoned roots are never reclaimed, the budget is not divided, and startup says so. Isolation still holds; what is given up is disk, not evidence.

The disk budget is a *total* across concurrent instances, not a per-process allowance. It is divided equally: with `N` live instances, each one enforces `total / N` on every write. A lone instance therefore gets the whole configured budget.

`N` is the number of instance roots currently held under advisory lock, read afresh at each budget decision. That is the entire input to the policy: it needs a *count*, never a peer's byte usage. Nothing is published between instances, so nothing can be stale, and there is no accounting transaction that could fail and hand out a grant no peer can see.

**Reading `N` when the directory turns hostile.** The instances directory is opened once at startup and enumerated through that retained descriptor rather than by path. A path lookup re-checks permissions on every call, so a later `chmod` — or replacing what the path names — would blind every subsequent count; a descriptor's access check happened at open time and keeps being honoured. This is what keeps the count exact across the realistic ways it used to be lost. If a read still fails, the count falls back to the highest count this process has already proved, which can only narrow a share, never widen one. If the very first read fails, so the process has no evidence about peers at all, it assumes four rather than assuming solitude: an instance that cannot see its siblings must not conclude it has none. A quarter of the total is still a usable share, so capture does not stall.

**The guarantee.** Every instance enforces `<= total / N` at every write, where `N` is the live count when it can be read and a conservative substitute when it cannot. Once each live instance has performed one operation since the most recent instance joined, the combined footprint is at most the total.

**The residual, stated precisely.** The bound above is exact whenever the count can be read. Two narrow cases remain where the combined footprint can exceed the total. First, if enumeration fails *through the retained descriptor* — an I/O-level failure, since permission and path changes no longer defeat it — and a peer joined since this process's last successful count, the fallback reports the older, smaller count. A peer that joined during a blackout is invisible to any process that cannot read the directory, so no fallback derived from remembered state can see it. Second, if more than four instances all start while unable to read the directory, the assumed count of four is too low; five such instances can reach `5T/4`. These two cases are not equally hard to provoke. The first requires enumeration to break at the descriptor level — an I/O fault, since permission and path changes no longer defeat a descriptor already held. The second only requires the directory to be unreadable *before* those instances start, which a permission change does cause, because an instance that never opened the directory has no descriptor to retain.

**The carve-out, stated honestly.** Reclaim is driven by operations, not by a timer. Krometrail runs no background trim or age-out scheduler: every reclaim walk, age-out included, happens inside some instance's own append, enforcement, or artifact-publication path. An instance that grew while the live count was lower therefore stays above its current share until its next operation, which is when it trims down to that share. Concretely: an instance alone with a total `T` may hold `T`; when a second instance starts, the first still holds `T` and combined usage can reach `3T/2`. If both then go idle, usage stays at `3T/2` indefinitely. It does not grow — the first instance's very next operation is judged against `T/2` and trims toward it — but nothing reclaims the excess while the process is idle. Krometrail does not claim the combined footprint converges on its own. Process exit releases the excess unconditionally: the bytes stop counting immediately, and a later start removes the abandoned recording cache. Removing it is best effort per start — a root whose lock cannot be taken at that moment is stepped over rather than retried, so reclamation may take more than one start. The bytes are disk, not evidence: nothing live references them and nothing depends on when they go.

**The accepted cost.** Two live instances each get `total / 2` even when one of them is idle, and a write larger than a share is refused however much disk is free. A policy that let a busy instance use whatever idle peers are not holding would need each peer's exact byte count at the instant of every write — a figure that is stale the moment it is read, because instances write independently. Predictability is the deliberate trade.

### Reclaim

Krometrail reclaims on both size and age, through one ordered walk. Abandoned instance roots go first, then generated artifacts, then browser events and segments in retention order.

Evidence older than the configured maximum age expires even when the store is well inside its budget, so a store does not accumulate until it reaches the budget wall and stay there. Reclaim also runs during a live session once usage crosses a high-water share of the instance's allowance, so a long session trims as it goes rather than degrading into permanent near-full pressure.

Reclaim is driven by operations, not by a timer. There is no background scheduler: every walk, age-out included, runs inside an append, an enforcement pass, or an artifact publication. Expired evidence is therefore removed on the store's next operation rather than at the instant it expires, and a store whose process has gone quiet retains what it holds until that process does more work or exits.

A segment backing an artifact published within a short grace window is skipped during budget pressure, so a freshly returned evidence link is not immediately invalidated. If every remaining segment is so protected, the grace is dropped rather than stalling capture, and the override is reported.

A time range can be pinned. Pinning protects every storage segment required to reconstruct that range, against both budget pressure and age-out. If pinned data consumes the entire budget, recording pauses before deleting protected evidence and reports the condition clearly.

### Retention configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `KROMETRAIL_DATA_DIR` | platform data directory | Root holding instance storage, browser profiles, and diagnostics. |
| `KROMETRAIL_DISK_BUDGET_BYTES` | 10 GB | Total budget shared across all live instances. |
| `KROMETRAIL_RETENTION_MAX_AGE_SECS` | 7 days | Age at which evidence expires regardless of budget. `0` disables age-out. |

Because evidence is instance-scoped, the maximum age is the main control over long-term growth; disabling it means a store only ever reclaims under budget pressure.

The status surface reports:

- configured budget;
- current usage;
- pinned usage;
- oldest retained time;
- newest retained time;
- requested `every_nth_frame` stride and observed capture cadence;
- recorded and dropped frames;
- whether eviction or recording is blocked.

Stopping a session flushes accepted frames and metadata before reporting completion.

## Temporal Ranges

A `SessionTime` is a session-relative monotonic nanosecond value.

A temporal range can be specified by:

- explicit session-relative time;
- explicit timestamps;
- interaction identifier;
- a window before and after an interaction;
- the most recent interaction;
- navigation or marker identifier;
- a source-frame range.

Natural anchors resolve to an explicit target and time range before artifact generation. When an interaction query omits an explicit range, Krometrail uses bounded pre-action context through the interaction lifecycle and post-action observation, plus bounded trailing context. Under `allow_partial`, an interaction or latest-interaction range that intersects retained capture may resolve to that exact intersection when its naturally derived edges extend beyond captured bounds. The response preserves the original requested range and interaction identity and reports affected-edge plus `partially_captured` warnings. Explicit ranges, `require_complete`, and wholly disjoint natural ranges remain exact failures. The resolved range is returned with every response.

Range resolution can also return an opaque session-scoped handle. Temporal artifact, event, source-frame,
pinning, and video requests accept either the complete resolved range or that handle. A handle resolves to the
same immutable range payload for the lifetime of the MCP process and fails explicitly after restart,
invalidation, or session deletion; it is a convenience reference, not an independent evidence authority.

The `resolve_temporal_range` operation performs only natural-anchor resolution and capture-quality assembly. It
returns the resolved range, a new range handle, and capture-quality evidence; it does not generate artifacts or
query browser events. The handle is accepted anywhere a temporal request accepts a resolved range.

Queries fail clearly when part or all of the requested range has been evicted, was never captured, belongs to a different target, or contains known capture gaps.

## Temporal Queries

The temporal-vision capability supports:

- inspect a range;
- resolve a natural temporal anchor into a range handle and capture-quality summary;
- inspect a window around an interaction;
- generate a temporal storyboard;
- generate a temporal difference map;
- generate supported region-focused artifacts;
- generate a bounded temporal video clip when the conditional capability is registered;
- list source frames and capture gaps;
- retrieve selected source frames;
- pin or unpin a range;
- report visual-change measurements;
- combine related outputs into a temporal debug bundle.

A temporal debug bundle is the primary investigation entry point. Its artifact work defaults to the visual epoch containing the effective anchor while preserving the complete resolved range, gaps, and original epoch identity. `epochs: "all"` explicitly expands acquisition across geometry transitions; direct artifact generation remains all-epoch by default. The bundle contains a concise text summary, artifact references, source-frame references, provenance, resolved timing, capture-quality warnings, and a compact deterministic selection of errors, failed requests, navigation, and browser events nearest major visual changes. Focused tools provide source frames, region artifacts, individual artifact variants, verbose events, and pin controls for progressive detail.

Concise bundle responses publish one primary compact artifact handle/resource and exact selected-epoch, available/unavailable outcome, and omitted outcome/resource counts. Expanded responses publish compact handles and resources for every generated outcome. Full responses retain complete generator, frame, and provenance structures. The primary artifact is inlined by default; `inline_images: false` retains the structured result and resource without reading pixels.

Large images and raw frame collections are returned by file or MCP resource reference. The response may include a context-sized primary image for immediate model inspection. Request and response bodies are drill-down evidence rather than default bundle content.

Resolved-order source-frame listings are paginated with an optional `offset`. Each page is truncated to its
`max_frames` limit and reports `omitted_frame_count` plus `next_offset` when another page is available. Explicit
frame selections and `fetch_source_frames` remain strict and cannot use pagination to bypass their limits.

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
- the session capture-configuration identity, including requested `every_nth_frame`, when the artifact comes from browser capture;
- resolved time range;
- ordered source-frame identifiers;
- omitted-frame count;
- known capture gaps;
- crop or mask;
- dimensions and scale;
- transformation parameters;
- algorithm version;
- whether the output is source-derived or inferred.

Video artifacts additionally record the presentation policy, presentation timestamps mapped to session time and ordered source frames, visible gap intervals, exact FFmpeg build and selected H.264 encoder, encoding parameters, container/media type, and output hash. A video is a derived presentation, not a replacement for its manifest or source frames. Known capture gaps are rendered explicitly rather than interpolated, and video export contains no audio.

An agent can retrieve the source frames behind any artifact while they remain retained.

## Errors and Degraded Operation

Boundary failures return structured errors containing:

- stable error code;
- plain-language explanation;
- affected session, target, interaction, or range;
- whether retry is safe;
- a concrete recovery action when one exists.

Deterministic limit failures name the bounded subject together with the actual
value and runtime limit, and include a fitting `try ≤` suggestion when one is
computable. Their retry advice is `never`; callers should change the named
input, selection, scale, crop, or target as directed by the recovery action.
Recovery labels describe the real state change that can make the operation
succeed: browser-state failures may be retryable after reload or navigation,
while a fixed resource ceiling is not made successful by repeating the same
request. A pre-dispatch interaction failure may retain a context anchor for
the target and operation, but it does not fabricate an interaction record.

Krometrail degrades explicitly:

- stale page references require a new snapshot;
- a disconnected browser enters reconnecting state;
- unrecoverable disconnection closes the session after flushing accepted data;
- a hidden target records a visibility gap;
- a saturated ingestion queue records dropped frames;
- an exhausted disk budget pauses capture when protected data prevents eviction;
- a dispatched interaction with unavailable post-action observation returns its interaction record as a degraded non-error result;
- invalid layout metrics can use the JavaScript-observed viewport size and report the fallback, while recovery-requiring metric failures name reload or navigation and are retryable after recovery;
- an unsupported CDP command reports the detected browser and protocol versions.

When the browser session has ended, its slot is reaped before a later `start_browser` call proceeds. Stopping an already-ended session reports successful cleanup. Last-page-close and ended-session failures direct the caller to `start_browser` for a new session.

Missing or unqualified FFmpeg does not prevent MCP startup; Krometrail omits the temporal-video tool surface. If the qualified executable becomes unavailable after startup, a video request fails with a stable encoder-unavailable error, bounded sanitized diagnostics, and a concrete recovery action.

## Local Data and Telemetry

Captured browser data remains local unless the user or connected agent explicitly reads an artifact through MCP.

Krometrail does not send usage telemetry, session contents, update checks, or derived artifacts to an external service by default.

Krometrail does not upload generated video to a model provider. The MCP host decides whether and how to pass a returned local video resource to a video-capable model.

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
- automatic comparison between interactions or sessions;
- inspection or control of an Electron Node main process;
- support for non-Chromium browser engines;
- bundled video encoding or audio capture;
- automatic detection that an MCP host or selected model can consume video;
- byte-identical video output across different FFmpeg builds or H.264 encoders.
