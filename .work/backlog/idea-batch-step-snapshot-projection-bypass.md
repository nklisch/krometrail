---
id: idea-batch-step-snapshot-projection-bypass
created: 2026-07-19
updated: 2026-07-19
tags: [agent-ux, browser]
---

Found in the 2026-07-19 fourth shakedown (v1.2.4 live): batch step results
bypass the concise projection bounding that standalone tools apply. A batch
[navigate → scroll → snapshot_page anchor=viewport] on Wikipedia "Web browser"
(4197 AX nodes) returned 828KB — step[2].result serialized the complete raw
node array (~813KB) even at default concise detail, while the identical
standalone snapshot_page call projects the bounded 48-target viewport ranking
(~13KB, geometry_omitted false, presentation_targets 431 honestly counted).
The final_observation in the same batch response was also correctly bounded
(~12KB) — only the per-step result embedding is unprojected.

Canonical-result-projection violation in the batch step projection path
(krometrail-mcp response.rs batch arm): step results should route through the
same detail-tiered projections as their standalone operations. Repro: any
batch containing snapshot_page on a large page blows the host token cap; hit
live on the fourth shakedown's first Wikipedia batch.
