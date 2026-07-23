---
id: story-exhaustive-cap-structured-error
kind: story
stage: implementing
tags: [temporal]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Structured recovery for the exhaustive-sampling cap error

## Brief

Requesting `sampling: "exhaustive"` on a range beyond the analysis plan cap
fails with an honest but bare string: "exhaustive analysis source plan: 2190
frames and 10271905920 decoded bytes exceeds limit 120 frames and 805306368
decoded bytes". It names the numbers but carries no structured recovery and no
diagnostics, in contrast to the region-filmstrip output-bound error whose
message states the exact remedy ("artifact raster height 7636 exceeds
output.max_height 2048; raise output.max_height").

Direction: route this refusal through the same structured error surface as
other bounded-refusal paths so the MCP error carries explanation plus recovery
("narrow the resolved range so at most 120 frames fall inside it, or use
uniform_bounded sampling which analyzes a bounded subset of any range") and
diagnostics correlation. Keep the exact plan numbers in the explanation.

## Acceptance criteria

- [ ] The exhaustive-cap refusal returns the structured error shape (
      explanation + recovery + diagnostics correlation id), with the frame and
      decoded-byte numbers preserved.
- [ ] Recovery text names both remedies: narrower range and uniform_bounded.
- [ ] A test pins the refusal shape at the cap boundary (120 passes, 121
      refuses with the structured error).
- [ ] Workspace gate green.
