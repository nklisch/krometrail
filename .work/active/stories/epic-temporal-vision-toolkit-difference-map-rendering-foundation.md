---
id: epic-temporal-vision-toolkit-difference-map-rendering-foundation
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-difference-map
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Rendering Foundation (Encoding Seam, Canvas, Font)

## Checkpoint

Establish the shared rendering seam every temporal-vision artifact renderer consumes: a deterministic PNG encoder with a pinned profile, a lazy SHA-256 output hash, a checked RGBA8 `Canvas` with drawing primitives, and a minimal hand-authored bitmap font for labels. This story introduces `png` and `sha2` as normal dependencies of `temporal-vision` and is the first code to land for the difference-map feature.

## Files (canonical shared-seam layout, matching the sibling `storyboard` design)

- `crates/temporal-vision/src/render.rs` (new — shared `Canvas` + drawing primitives + `RenderedArtifact` + `ImageEncoding`)
- `crates/temporal-vision/src/render/font.rs` (new — checked-in bitmap font)
- `crates/temporal-vision/src/encode.rs` (new — deterministic PNG encoder + SHA-256 hashing)
- `crates/temporal-vision/src/lib.rs` (add `mod render; mod encode;` and explicit exports for `ImageEncoding` and `RenderedArtifact`)
- `crates/temporal-vision/Cargo.toml` (add `sha2.workspace = true` and `png.workspace = true`)
- `Cargo.toml` (add `png` to `[workspace.dependencies]`; pin an exact patch in `Cargo.lock`)

> **Parallel-feature policy:** If the sibling `storyboard` or `region-filmstrip` feature has already landed `render.rs` / `render/font.rs` / `encode.rs`, this story REUSES them verbatim and contributes nothing new to those files. If no renderer has landed yet, this story lands the canonical seam in the layout above for all four artifact features to share. The orchestrator sequences implementation so the seam lands exactly once.

## Public surface (exact signatures)

```rust
// render.rs
stable_registry! {
    pub enum ImageEncoding { Png => "png" }
}

pub struct RenderedArtifact { /* encoding, bytes, hash */ }
impl RenderedArtifact {
    pub(crate) fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Self>;
    pub const fn encoding(&self) -> ImageEncoding;
    pub fn bytes(&self) -> &[u8];
    pub const fn output_hash(&self) -> OutputHash;
}

pub(crate) struct Canvas { /* dimensions, rgba */ }
impl Canvas {
    pub(crate) fn new(dimensions: PixelDimensions, background: [u8; 4]) -> Result<Self>;
    pub(crate) const fn dimensions(&self) -> PixelDimensions;
    pub(crate) fn as_rgba(&self) -> &[u8];
    pub(crate) fn fill_rect(&mut self, rect: PixelRect, color: [u8; 4]) -> Result<()>;
    pub(crate) fn put_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) -> Result<()>;
    pub(crate) fn blit_rgba(&mut self, origin: (u32, u32), panel: PixelDimensions, rgba: &[u8]) -> Result<()>;
    pub(crate) fn draw_text(&mut self, origin: (u32, u32), text: &str, color: [u8; 4]) -> Result<()>;
    pub(crate) fn draw_gradient(&mut self, rect: PixelRect, start: [u8; 4], end: [u8; 4]) -> Result<()>;
}

// render/font.rs (checked-in 6×N monochrome set; exact glyph dimensions match the
// shared seam chosen by the first renderer to land — storyboard proposes 6×10)
pub(crate) const GLYPH_WIDTH: u32 = 6;
pub(crate) const GLYPH_HEIGHT: u32 = 8; // reconcile to 10 if storyboard lands first
pub(crate) const fn glyph(character: char) -> Option<&'static [&'static [u8; 8]]>;
```

## Implementation notes

- `encode_png` builds a `png::Encoder` with `Compression` and `FilterType` pinned to fixed values documented inline; the buffer passed to the writer is exactly `width · height · 4` opaque RGBA8 bytes. All dimension arithmetic is checked and rejects overflow with `ResourceLimitExceeded`. SHA-256 is computed once via `sha2::Digest` and stored alongside the bytes; `output_hash()` is a `const` accessor.
- `Canvas` rejects out-of-bounds rectangles and blits with `InvalidRegion`. `draw_text` upper-cases its input and renders unsupported characters as a blank glyph cell; it never panics.
- The font is a `const` 6×8 monochrome set covering A–Z, 0–9, space, and the punctuation/symbols the artifact labels require (`. : + - / % >`). No runtime initialization, no allocation.

## Acceptance evidence

- Identical RGBA8 inputs produce byte-identical PNG output across repeated `encode_png` calls, and `output_hash()` equals an independently computed SHA-256 of `bytes()`.
- `Canvas` fill/blit/text/gradient operations are deterministic and reject out-of-bounds rectangles with `InvalidRegion`.
- The glyph set renders every character used by the difference-map labels; unsupported characters degrade to a blank cell rather than panicking.
- `cargo tree -p temporal-vision --edges normal` adds only `png`, `sha2`, and their transitive pure-Rust dependencies.
- `cargo fmt -p temporal-vision -- --check`, locked package check/test/clippy pass.

## Ordering constraints

No upstream story dependency. Downstream stories (`change-accumulation`, `panel-rendering`, `public-contract-tests`) build on this seam. The implementer may run this in parallel with the `change-accumulation` story except for the shared `lib.rs` module list, which one owner should land coherently. Coordinate with the sibling `storyboard` / `region-filmstrip` features so the shared seam (`render.rs`, `render/font.rs`, `encode.rs`) lands exactly once in the canonical layout; reuse verbatim if another renderer lands it first.

## Implementation notes

- Execution capability: raised/high; shared renderer integration affects deterministic public evidence.
- Review weight: standard (autopilot caller).
- Files changed: substrate checkpoint only; storyboard already landed `artifact.rs`, `render.rs`, `render/canvas.rs`, `render/font.rs`, and `encode.rs` in commits `d0b4185` and `5016d8d`.
- Tests added/removed: none; the existing shared-seam tests and 33 package tests pass.
- Simplification: reused the RGB8 `GeneratedArtifact`/`EncodedImage`/`Canvas` seam exactly once instead of adding the designed parallel RGBA8 `RenderedArtifact` abstraction.
- Discrepancies from design: the landed seam is RGB8 rather than RGBA8 and exposes hashes through `ArtifactManifest`; difference-map rendering will adapt to it without changing storyboard.
- Adjacent issues parked: none.
