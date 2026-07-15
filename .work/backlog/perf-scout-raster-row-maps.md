---
id: perf-scout-raster-row-maps
created: 2026-07-15
tags: [perf, idea, memory]
idea_origin: scout
idea_lens: memory-locality
idea_borrowed_from: software rasterizers
idea_priority: worth-a-look
idea_location: crates/temporal-vision/src/render/canvas.rs:102
idea_source: scout
---

# Precompute raster coordinate maps and write rows directly

**Location**: `crates/temporal-vision/src/render/canvas.rs:102` · **Lens**: memory-locality · **Borrowed from**: software rasterizers · **Priority**: worth-a-look
**Leverage**: Medium-High · **Applicability**: Plausible · **Cost**: Low

> ⚠️ Unvalidated hypothesis — no measurement has been done. This is a candidate to investigate, not a proven win.

**The idea**: Precompute x/y source maps once per geometry and write validated destination row slices directly for tiles and panels.

**Why it might help**: This might remove repeated integer division, index reconstruction, bounds checks, and per-pixel destination arithmetic.

**Validate by**: Profile render-only stages and require exact pixels/PNG hashes across layouts and aspect ratios.

**Risk**: Contain-fit rounding and annotation overlap must remain exact.
