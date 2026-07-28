# temporal-vision

Deterministic temporal visual analysis: turn a sequence of frames into
**storyboards, difference maps, motion-history images, and region
filmstrips** — with a fully self-describing provenance manifest for every
artifact.

Pure Rust, synchronous, integer-only pixel math. No async runtime, no I/O,
no rendering dependencies — the same input always produces byte-identical
output, on any platform, with any thread count.

## What it does

Given a `FrameSequence` (RGBA8 frames + timestamps, optional markers and
declared gaps), temporal-vision produces:

- **Storyboards** — 3–12 *informative* frames selected by change analysis
  (not uniform sampling), composited into one labeled montage. Each selected
  frame carries the reasons it was chosen; displaced candidates are named as
  omitted anchors.
- **Difference maps** — two-panel heatmaps: how often each pixel changed
  (frequency) and when (spectral timing), relative to a reference frame.
- **Motion-history images** — one image summarizing where movement happened
  over a window, decay-weighted by recency, gap-aware (decay resets across
  declared timeline gaps instead of smearing).
- **Region filmstrips** — the same crop across evenly-spaced frames, either
  fixed or **tracked**: per-frame moving crops that follow a region you
  supply (e.g. the projected screen position of an object), with
  out-of-image areas rendered as honest padding.

Every artifact ships with a manifest distinguishing three frame
populations — **source** (recorded) ⊇ **analyzed** (decoded) ⊇
**selected** (rendered) — plus algorithm descriptors, parameters,
normalization steps, and a SHA-256 output hash. A generator cannot get its
provenance counts wrong; the shapes are checked.

## Built for AI agents

temporal-vision is designed as the visual-evidence layer for LLM-driven
debugging tools. It currently powers two MCP (Model Context Protocol)
servers:

- **[Krometrail](https://github.com/nklisch/krometrail)** — gives agents
  temporal visual awareness of web pages: storyboards and difference maps
  of browser sessions for diagnosing flicker, layout shifts, and transient
  UI defects.
- **[Theatre/Stage](https://github.com/nklisch/theatre)** — gives agents
  spatial + visual awareness of running Godot games: clip storyboards,
  motion history, and node-following filmstrips for diagnosing spatial and
  rendering bugs.

Why it fits agent consumers:

- **Token efficiency is the design constraint.** One montage image
  amortizes a whole window of frames; manifests are compact JSON with
  exact counts and continuation-friendly structure instead of frame dumps.
- **Deterministic output → content-addressed caching.** Identical inputs
  yield identical bytes, so hosts can cache artifacts by hash and serve
  repeat queries for free.
- **Honesty is rendered, not asserted.** Missing frames become padded
  tiles, gaps are declared and gap-aware, labels say what the artifact
  actually shows (e.g. `TRACKING <label> | PER-FRAME REGION`), and
  truncation is always explicit. An agent can trust what it reads.
- **Borrowed-slice input.** `Frame<Id, &[u8]>` accepts raw pixel buffers
  without copying; IDs are caller-owned generics, so hosts key frames to
  their own timelines.

## Design properties

- **Deterministic parallel merge** — custom `std::thread::scope`
  parallelism (no Rayon), capped workers, in-order merge; output bytes are
  identical regardless of worker count.
- **Integer-only pixel math** — sRGB→linear via LUT, weighted-channel
  change classification; no float drift across platforms.
- **Bounded everything** — processing, render, and output limits are
  explicit; exhaustion is a typed error, never an OOM.
- **Plan/render separation** — `MotionHistoryPlan`, `RegionFilmstripPlan`,
  `StoryboardSelection`, `DifferenceMapData` can be consumed directly and
  rendered by your own UI (or skip the built-in renderer entirely).

## Example

```rust,ignore
use temporal_vision::*;

// Build a validated frame sequence (caller-owned ids, borrowed pixels ok).
let seq = FrameSequence::new(frames, markers, gaps, None, None)?;
let normalized = normalize_sequence(&seq, &NormalizationParameters::default())?;

// Generate a storyboard: informative frames, one montage, full manifest.
let artifact = generate_storyboard(
    artifact_id,
    None,
    &seq,
    &normalized,
    StoryboardParameters::default(),
)?;
let png_bytes = artifact.storyboard().image().bytes();
let manifest = artifact.storyboard().manifest(); // provenance + selection reasons
```

## Releases

temporal-vision is versioned and published independently of the krometrail
workspace. Releases are cut from tags (`temporal-vision-vX.Y.Z`) and
published to crates.io via GitHub OIDC Trusted Publishing; see
[docs/RELEASING.md](../../docs/RELEASING.md) for the bump policy.

## License

MIT
