# Temporal Visual Evidence

## Purpose

Krometrail converts moving browser activity into still visual artifacts that multimodal coding agents can inspect.

These artifacts do not replace the recorded frames. They provide compact views of a selected interval, expose different kinds of temporal behavior, and let the agent decide which source evidence requires closer inspection.

The same visual vocabulary applies to browser, editor, game-engine, desktop, and other timestamped frame sources.

## Evidence Classes

Krometrail distinguishes three classes of visual output.

### Source frames

A source frame is an image received from the capture system and stored without visual transformation.

Source frames are the authoritative visual record, subject to the capture cadence and gaps reported by the recorder.

### Source-derived artifacts

A source-derived artifact selects, crops, scales, combines, thresholds, colors, or rearranges recorded frames.

Examples include:

- storyboards;
- difference maps;
- region filmstrips;
- motion-history images;
- temporal slices.

These outputs are deterministic and traceable to their source frames, but they are lossy. They can omit, obscure, exaggerate, or visually merge details.

### Inferred artifacts

An inferred artifact estimates information that is not directly represented in one captured frame.

Examples include:

- optical-flow vectors;
- tracked object paths;
- inferred direction or velocity;
- logical element correspondence;
- anomaly labels.

Inferred artifacts identify their method and confidence. They are never presented as raw observations.

## Shared Artifact Contract

Every artifact includes:

- artifact type;
- selected target or generic frame source;
- resolved start and end times;
- visible time direction;
- source-frame identifiers;
- source-frame count;
- omitted-frame count;
- known capture gaps;
- crop, mask, and scaling parameters;
- transformation parameters;
- algorithm version;
- evidence class;
- a machine-readable provenance manifest.

Every visual artifact displays:

- a title;
- start and end offsets;
- an explicit time-direction indicator;
- a legend for color, intensity, and symbols;
- a visible warning when the source interval contains capture gaps;
- enough context to identify the relevant page or region.

Color is never the only indication of time or change. Labels, ordering, intensity, patterns, or symbols provide redundant meaning.

## Input Sequence

The temporal visual crate accepts an ordered sequence:

```text
FrameSequence
  frames
    id
    timestamp
    dimensions
    pixels
  markers?
    timestamp
    label
    kind
  region?
  mask?
  gaps?
```

Frames in one sequence use a common coordinate space and dimensions.

A viewport resize, orientation change, device-scale change, or incompatible crop divides the input into separate visual epochs unless the caller explicitly requests a declared normalization. Artifacts do not silently stretch incompatible frames into alignment.

The temporal debug bundle keeps the complete resolved range and gap provenance but defaults artifact work to the visual epoch containing its effective anchor. If the anchor falls between epoch spans, the nearest epoch is selected and an equal-distance tie chooses the earlier epoch. Callers investigating geometry transitions can request all epochs explicitly. Direct artifact generation continues to operate over every compatible epoch in the resolved range.

Gaps divide continuous measurements. The renderer can place segments on one timeline, but calculations do not treat missing time as observed stability.

## Normalization

Source images can be decoded into a common pixel representation for analysis.

Permitted source-derived normalization includes:

- color-space conversion;
- alpha compositing against a declared background;
- integer scaling with recorded parameters;
- fixed cropping;
- light denoising or thresholding with recorded parameters.

Geometric registration, perspective correction, content-aware alignment, and generated pixels are inferred transformations and require explicit labeling.

The original encoded frame remains available.

## Visual-Change Measurements

The crate calculates direct measurements used for frame selection and artifact rendering:

- absolute pixel difference;
- changed-pixel proportion;
- changed-region bounds;
- luminance difference;
- color difference;
- perceptual frame distance;
- time since the preceding captured frame.

Measurements are descriptive. A high change score does not itself mean jitter, instability, or error.

Noise thresholds are configurable and appear in provenance. Default thresholds reduce encoding and anti-aliasing noise without claiming to remove all irrelevant change.

## Temporal Storyboard

The temporal storyboard is the primary visual artifact.

It presents representative source frames in explicit chronological order.

```text
Interaction: click “Open panel”
Range: -150 ms to +1,250 ms

BEFORE       FIRST CHANGE       PEAK CHANGE       REVERSAL CANDIDATE       FINAL
-104 ms      +33 ms             +201 ms           +367 ms                  +1,184 ms
[frame]      [frame]            [frame]            [frame]                  [frame]

────────────────────────────── time ───────────────────────────────────────────▶
```

### Required anchors

When available, selection preserves:

- the last source frame before the range anchor;
- the first source frame after the anchor;
- the first measurable visual change;
- the frame with the greatest difference from the pre-action baseline;
- the final retained frame.

Interaction, navigation, marker, and capture-gap boundaries can also be mandatory anchors.

### Change-aware selection

Remaining tiles are selected from captured source frames using deterministic visual-change measurements.

Selection favors:

- local peaks in frame-to-frame change;
- changes in the trend of visual difference;
- appearance or disappearance of changed regions;
- frames that add information not already represented by selected neighbors;
- temporal coverage of long intervals.

“Reversal candidate” or similar labels describe a selection reason, not a diagnosed defect.

The default storyboard contains no more than eight tiles. Callers can request between three and twelve. When text or fine detail becomes unreadable, the artifact provides region crops or source-frame references instead of adding smaller tiles.

### Layout

Each tile includes:

- session-relative timestamp;
- offset from the epoch-local render anchor, while the original semantic query anchor remains in resolved-range provenance;
- source-frame identifier;
- marker labels that intersect its time;
- gap indication when applicable.

The storyboard preserves source aspect ratio. It does not use decorative borders or backgrounds that can be mistaken for page content.

## Temporal Difference Map

A temporal difference map shows where pixels changed during an interval.

The default artifact contains three coordinated panels:

1. **Reference** — the source frame used as spatial context.
2. **Change frequency** — how often each pixel or region changed.
3. **Change timing** — when recorded changes occurred.

```text
REFERENCE              CHANGE FREQUENCY       CHANGE TIMING
[source frame]         [grayscale map]        [time-colored map]

No change  ░▒▓█  Frequent change
Early  ───────────────────────────────▶  Late
```

### Frequency panel

Brightness represents the accumulated number or magnitude of thresholded changes. The legend reports whether the map represents count, magnitude, or normalized frequency.

### Timing panel

A declared time palette represents the weighted time of observed changes. Numeric start, midpoint, and end labels accompany the palette.

Pixels that change repeatedly across widely separated moments receive a repeated-change indicator rather than a falsely precise single timestamp.

### Interpretation

A difference map can expose:

- regions of repeated change;
- movement paths;
- flicker concentrated in one area;
- unexpected changes outside the interaction target;
- visually quiet periods.

It does not explain why a region changed. Scrolling, caret blinking, video, anti-aliasing, loading indicators, and legitimate animation can all contribute.

## Region Filmstrip

A region filmstrip presents one visual region across time at readable scale.

```text
REGION IN CONTEXT       -40 ms    +20 ms    +80 ms    +140 ms    +220 ms
[locator image]         [crop]    [crop]    [crop]    [crop]     [crop]
```

The artifact includes a locator image showing the region within a full source frame.

A region can be:

- fixed in viewport coordinates;
- fixed in source-image coordinates;
- supplied independently for each frame by a declared tracking method.

A fixed region does not follow a logical element. Tracked regions are inferred and state the tracking method and confidence.

Filmstrip crops use consistent output scale. Padding is explicit when a region extends beyond a frame.

## Motion-History Image

A motion-history image accumulates recently changed pixels over one spatial reference.

Recent change appears stronger than older change. A visible decay legend maps intensity to relative time.

The default rendering combines:

- a subdued source-frame reference;
- a motion-history layer;
- changed-region outlines;
- start and end labels.

Motion history is useful for seeing paths, repeated traversal, broad oscillation, and the spatial extent of movement.

It does not inherently prove direction. Direction arrows, velocity, and object trajectories require inferred motion analysis and do not appear in a source-derived motion-history image.

Text and detailed interface regions can become unreadable when many states overlap. The artifact therefore links to a storyboard and region filmstrip for disambiguation.

## Before/During/After Composite

A temporal debug bundle includes a simple orientation composite:

```text
BEFORE                 DURING                 AFTER
[source frame]         [selected frame]       [source frame]

Anchor: click I-52     During selected at maximum baseline difference
```

“During” is selected by a declared rule and identifies its source frame. It is not generated or averaged.

The composite provides a low-complexity entry point before the agent inspects denser temporal artifacts.

## Temporal Video Clip

A temporal video clip is an optional source-derived presentation of a resolved interval. It is generated only when Krometrail qualified a user-installed FFmpeg implementation at MCP startup. It does not replace still artifacts, source-frame access, or the temporal debug bundle.

The initial container and codec contract is MP4/H.264 without audio. Output duration, dimensions, presentation frame count, encoded bytes, CPU concurrency, wall-clock deadline, and external-process lifetime are bounded. Frames from incompatible visual epochs are not silently stretched together.

Two presentation policies serve different inspection needs:

- **real time** preserves the resolved interval's relative timing within declared output bounds;
- **model optimized** lengthens selected meaningful states so provider-side sparse sampling or keyframe selection is less likely to omit them.

Both policies are derived views. The model-optimized policy does not imply that held frames lasted that long in the recorded session. Its manifest maps every presentation interval back to session time and ordered source-frame identifiers.

Known capture gaps become explicit labeled gap slates whose duration and mapping are recorded. The encoder never interpolates motion or invents intermediate observations across a gap. The clip manifest records its presentation policy and plan, source identities, gaps, epoch handling, exact FFmpeg build, selected H.264 encoder, encoding parameters, media type, and output hash.

The presentation plan is deterministic for identical inputs and parameters. Encoded bytes are deterministic only for the exact recorded external encoder identity; Krometrail makes no byte-equality claim across FFmpeg builds, platforms, or H.264 implementations.

## Temporal Debug Bundle

The default bundle combines complementary evidence rather than relying on one image.

It contains:

1. a concise textual header;
2. before/during/after composite;
3. temporal storyboard;
4. temporal difference map;
5. source and artifact references;
6. capture-quality information;
7. related interaction markers and a compact deterministic selection of errors, failed requests, navigation, and browser events nearest major visual changes.

A typical header reports:

```text
Observed target T-2 from -150 ms to +1,250 ms around interaction I-52.

Capture:
- 31 source frames
- median observed cadence: 29 fps
- 0 declared gaps

Visual measurements:
- first detected change: +33 ms
- greatest baseline difference: +201 ms
- changed area peaked at 18% of the viewport
- repeated change concentrated in region R-4

These are visual measurements, not a diagnosis.
```

The primary image included directly in an MCP response is sized for model inspection. Full-resolution artifacts and source frames remain available as resources.

## Progressive Detail

An agent can move from compact evidence to raw evidence:

```text
summary
  → orientation composite
    → storyboard and difference map
      → region filmstrip
        → selected source frames
          → complete retained frame range
```

Every level references the level beneath it. The agent does not need to request the complete recording to verify a generated view. Compact levels never inline the complete retained identifier enumeration; they present bounded slices with exact omission counts and name the drill-down authority for the rest.

## Capture Gaps

The capture path receives a CDP frame, immediately acknowledges it, and only then attempts bounded handoff. A failed enqueue is an explicit capture gap after acknowledgement; acknowledgement does not mean that the frame was retained. Ack latency is measured from returned frame to acknowledgement completion, excluding the receive wait and any later handoff or persistence work.

A pending capture-geometry refresh is metadata uncertainty, not missing visual evidence. Frames remain retained with the last established viewport and a `viewport_metadata_incomplete` warning until refresh completes; no capture gap is declared solely for beginning or completing that refresh.

`partially_captured` means that retained frames do not cover the full requested interval. It is an
evidence-coverage statement, not a claim that capture loss occurred: a requested tail may simply extend
past the newest retained frame while the session is still live. When guarded session-time evidence proves
that the requested end has not yet elapsed, `requested_end_not_yet_elapsed` refines that live tail. An
elapsed idle tail keeps `partially_captured`, because durable retained evidence cannot distinguish a page
that stopped changing from a capture stream that has not yet declared a gap.

Artifacts never interpolate across an undeclared capture gap.

A gap appears:

- in the textual summary;
- on the visual timeline;
- between affected storyboard tiles;
- in the provenance manifest.

An interval containing a gap can still produce artifacts, but the response does not claim that unseen behavior did not occur.

## Markers

Markers can represent:

- agent action start, dispatch, completion, and observation;
- navigation;
- browser lifecycle;
- console or network events;
- user annotations;
- generic caller-defined events.

The temporal visual crate treats marker kinds as caller-provided labels. It does not depend on browser-specific marker types.

Markers can affect frame selection but do not alter source pixels unless the renderer adds a clearly separated annotation area.

## Provenance

A provenance manifest is sufficient to reproduce an artifact while its source frames remain available.

```text
ArtifactManifest
  artifact_id
  artifact_kind
  evidence_class
  algorithm
  algorithm_version
  source_frame_ids        # ordered source-frame identifiers
  analyzed_frame_ids      # frames that contributed to the result
  selected_frame_ids      # frames the output renders or references
  omitted_frame_count     # source - analyzed: contributed nothing
  range
  markers
  gaps
  region
  normalization
  parameters
  output_dimensions
  output_hash
```

A manifest names three distinct frame populations, because "what evidence was
examined?" and "what does the picture show?" are different questions:

- `source_frame_ids` — every frame retained in the epoch.
- `analyzed_frame_ids` — the frames that actually contributed to the result. For
  an analysis artifact this is the sampled set; for a storyboard it is the frames
  read. Decoding a frame is not the same as consuming it: a filmstrip's tiles are
  chosen by position, so a decoded frame that backs no tile and is referenced
  nowhere contributed nothing and is counted as omitted, not as analyzed. Each
  generator declares which of the two shapes it has, and the manifest derives all
  three populations from that one declaration.
- `selected_frame_ids` — the frames the output renders or directly references. A
  difference map references one frame while analyzing many.

`omitted_frame_count` is `source - analyzed`: frames that contributed nothing.
Frames that were analyzed but not rendered are `analyzed - selected`, derivable
by any reader. Conflating these two is why a difference map that analyzed 93 of
474 frames once reported 473 omissions.

An analysis artifact that dropped frames must disclose its sampling. Every part
of that disclosure is validated, not only its counts: the sampling mode and
spacing must name a scheme these generators actually produce, the analyzed source
indices must be present, strictly increasing, in range, and as many as the
analyzed count, and no unrecognised field may ride along. Validation runs when the
manifest is constructed and again when it is deserialized. An artifact that
analyzed every frame emits no sampling block at all, so the block's presence is
itself the signal that sampling occurred.

Rendered annotations are derived from the manifest. Machine-readable and visible labels cannot disagree without failing artifact generation.

## Determinism

For identical decoded pixels, timestamps, markers, gaps, parameters, and algorithm version, the crate produces identical measurements, frame selections, manifests, and output pixels.

Parallel execution does not change selection or rendering order.

## Non-Diagnostic Posture

The visual crate describes observed change. It does not assert:

- that motion is erroneous;
- that a change is flicker;
- that an element reversed direction;
- that one event caused another;
- that a transition is smooth;
- that a patch fixed a defect.

Krometrail can present measurements and correlated markers. The coding agent interprets the evidence.

## Inferred Analysis

Inferred analysis is isolated from source-derived rendering.

An inferred result includes:

- method;
- implementation version;
- input frames;
- confidence or quality measure;
- known failure conditions;
- visualization parameters.

Source-derived artifacts remain available alongside inferred overlays so the agent can verify the estimate.

## Accessibility and Model Readability

Artifacts are optimized for inspection rather than decoration.

They use:

- large readable labels;
- explicit chronological ordering;
- limited tile counts;
- redundant time encodings;
- consistent legends;
- high contrast between annotation and page content;
- region crops when full-page detail is too small;
- concise accompanying text.

Artifact quality is an empirical question. [EVALUATION.md](EVALUATION.md) defines how presentation choices are tested with current multimodal agents.
