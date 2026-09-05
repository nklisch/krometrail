---
id: idea-mcp-scroll-delta-simplification
created: 2026-08-15
updated: 2026-08-15
tags: [agent-ux]
---

# Flattened Scroll Delta Wire Schema

## The Finding
The `scroll` MCP tool schema requires nested `value` wrapping inside `delta`:
```json
{
  "delta": {
    "kind": "by_offset",
    "value": {
      "x": 0.0,
      "y": 600.0
    }
  }
}
```

When agents attempt to scroll, they overwhelmingly emit `{ "delta": { "kind": "by_offset", "x": 0, "y": 600 } }` or `{ "delta": { "x": 0, "y": 600 } }`, which fails with `missing field value`.

## Fix Direction
1. In `crates/krometrail-mcp`, deserialize `ScrollDeltaWire` so that `by_offset` coordinates (`x`, `y`) can appear directly in the `delta` object alongside `kind`, or default `kind: "by_offset"` when `x` or `y` are present.
2. Support shorthand `{ "x": 0, "y": 600 }` or `{ "delta": { "x": 0, "y": 600 } }` without requiring the `kind: "by_offset"` enum discriminator.
