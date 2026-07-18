# Visual evidence semantics

Use this reference when choosing or interpreting temporal output. Krometrail describes observed local
browser evidence; it does not diagnose defects or infer causes.

## Evidence classes

## Economical evidence order

- A state-changing operation's returned live evidence is the cheapest confirmation of its immediate
  result. Do not automatically follow it with another screenshot.
- `observe_live` is a new explicit current-state sample, useful when the prior observation was
  degraded, time has passed, or the caller needs all current evidence parts together.
- Retained source frames and derived artifacts answer historical and transient questions. They are
  not interchangeable with either live observation point.

Check retained capture health before making a temporal claim. A capture warning can coexist with a
successful browser action; it limits history without undoing the action.

### Current live evidence

- **Page state** reports URL, title, viewport, selection, navigation, and related status.
- **Structured snapshot** exposes an accessibility-oriented tree with actionable references. It is
  useful for targeting and semantics, but it is not a pixel record and does not represent every
  canvas, video, WebGL, or custom-rendered detail.
- **Screenshot** records the requested current viewport, page, element, or region.
- **Live observation** combines current page state, snapshot, and screenshot at one observation point.

A final live observation can conceal a state that appeared and disappeared before observation. Use
temporal evidence only when the question concerns the interval.

### Source frames

Source frames are retained images received from the capture system without visual transformation.
They are the authority for what Krometrail visually observed, subject to:

- Chrome's damage-driven screencast behavior;
- the session's relative `every_nth_frame` request;
- target visibility and local rendering/system load;
- bounded ingestion, persistence, retention, and declared capture gaps.

A capture ordinal establishes local order for acknowledged frame events; it does not prove Chrome
emitted or Krometrail received every rendered frame.

### Source-derived artifacts

| Artifact | What it makes visible | What it does not establish |
|---|---|---|
| **Before/during/after** | A simple orientation view using identified source frames around an anchor | That the selected “during” frame contains every important intermediate state |
| **Storyboard** | Representative frames in chronological order, favoring anchors and informative visual changes | Complete frame-by-frame history; labels such as “reversal candidate” are selection reasons, not diagnoses |
| **Difference map** | Where change accumulated, its magnitude/frequency, and changed-region bounds | Why pixels changed; scrolling, video, caret blink, anti-aliasing, and legitimate animation can contribute |
| **Region filmstrip** | One fixed declared region across time at readable scale, with locator context | Logical element tracking unless the response explicitly declares a tracking method; a fixed crop may contain different content after layout movement |
| **Motion history** | Spatial extent, repeated traversal, and recency-weighted accumulated change | Inherent direction, velocity, object identity, or error |

Artifacts are deterministic and traceable for identical decoded pixels, times, markers, gaps,
parameters, and algorithm version. They remain lossy: selected views can omit, merge, obscure, or
visually exaggerate details.

## Temporal debug bundle

`temporal_debug_bundle` is a convenient compact combination, not a mandatory first step. Depending on
availability and request policy, it can include:

- a resolved interaction or time range and textual summary;
- before/during/after orientation;
- storyboard and difference-map references;
- selected source and artifact references;
- visual measurements and capture-quality warnings;
- nearby interactions, navigation, console/exception, and failed-request context.

The bundle favors context-sized evidence. Its compact artifact handles summarize identity, type,
geometry, hash, and frame counts. Read a handle's `manifest_uri` resource for the exact full manifest
when a claim depends on ordered source and selected frame IDs, omissions, gaps, normalization, or
generator parameters. Full-resolution images and exact source frames remain behind their adjacent MCP
resource links. If a claim depends on fine text or an exact intermediate state, retrieve that linked
artifact or source frame rather than relying on the inline preview.

## Progressive detail

Choose the least detail that answers the question, then drill down only where uncertainty remains:

- Artifact images, artifact manifests named by `manifest_uri`, and exact source-frame reads use the
  `krometrail://evidence/...` MCP resource links returned by tools. They are resource reads, not
  separate tool calls.
- `list_source_frames` shows ordered retained frame metadata and availability for a resolved range.
- `fetch_source_frames` returns a selected batch.
- `generate_region_filmstrip` isolates a declared region without requiring a full-frame artifact to
  make fine detail legible.
- `query_browser_events` provides more chronological event detail than the compact bundle selection.

Every level should preserve references to its underlying evidence. Missing, evicted, or corrupted
source data limits reproducibility and must remain visible in the conclusion.

## Ranges, gaps, and epochs

Temporal requests can resolve from an interaction, recent interaction, marker/navigation anchor,
session-relative time, explicit timestamps, or source-frame range. Use the resolved range returned by
Krometrail rather than assuming the natural anchor mapped to a particular timestamp.

A viewport resize, orientation change, device-scale change, or incompatible crop can divide evidence
into visual epochs. Krometrail does not silently stretch incompatible frames into one coordinate
space. A gap also divides continuous measurements: artifacts may show segments on one timeline, but
missing time is not observed stability.

A dark or partial live screenshot is not evidence that the action failed. Request one bounded fresh
`observe_live` sample. If degradation persists, preserve its correlation identifier and collect only
the targeted sanitized diagnostic fields described by the main skill.

## Provenance checklist

Before making a strong visual claim, check the response or manifest for the relevant:

- session and target identity;
- resolved start/end and anchor;
- ordered source and selected frame identifiers;
- source-frame and omitted-frame counts;
- capture cadence configuration and known gaps;
- crop, region, mask, dimensions, and normalization;
- transformation parameters and thresholds;
- evidence class, algorithm name/version, and output hash.

State the limitation when any field that matters to the claim is unavailable.

## Retention pins

Pinning protects the storage segments required to reconstruct a resolved range. It is useful while a
longer investigation still needs raw evidence. It also consumes the bounded storage budget and can
pause recording when protected data prevents eviction. Query the pin state when preservation matters,
and unpin the exact range after the need has passed.
