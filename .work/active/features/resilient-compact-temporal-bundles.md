---
id: resilient-compact-temporal-bundles
kind: feature
stage: drafting
tags: [agent-ux, visual]
parent: null
depends_on: [truthful-screencast-geometry]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Resilient compact temporal bundles

## Brief

Make the default temporal debug bundle a reliable, context-sized investigation entry point at realistic capture sizes. A request extending slightly beyond the newest captured frame currently fails with `not_found` but provides no recovery or safe retry guidance. A gap-free 53-frame, 1200×705 CSS, DPR 2 interval currently resolves yet loses all default artifacts to the decoded-sequence memory limit. A successful 52-frame bundle can inline roughly 44,000 tokens of complete artifact manifests even though canonical resource links already support progressive provenance access.

Preserve the stable 1.x range, artifact, and provenance contracts while making capture-edge errors actionable, fitting default high-DPI bundle work within bounded resources, and projecting only context-sized artifact metadata inline. Full manifests and source evidence must remain available through canonical resources.

## Reproduced findings

- A click-relative request for 500 ms before and 6 seconds after extended about 1.06 seconds beyond the newest captured frame. `AllowPartial` still returned `not_found` with null recovery and `retry: never`; 4.9 seconds after succeeded.
- A 5.4-second, 53-frame DPR-2 interval returned `resource_limit_exceeded` for decoded sequence bytes with no recovery guidance.
- A successful five-second, 52-frame bundle generated nine resources but expanded to roughly 44,000 structured-response tokens because every full artifact manifest was repeated inline.

## Simplification opportunity

Use the retained-range resolver and canonical resource boundary as the two authorities: one place should describe the nearest valid time bounds, and one compact artifact projection should link to full persisted provenance instead of maintaining two equally verbose delivery paths.
