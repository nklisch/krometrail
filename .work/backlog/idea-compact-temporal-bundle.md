---
id: idea-compact-temporal-bundle
created: 2026-07-18
updated: 2026-07-18
tags: []
---

During the post-1.1.0 cross-surface pass, a successful default `temporal_debug_bundle` around a GitHub page scroll returned useful gap-free evidence (15 retained frames, three artifact/resource pairs, and a reusable range handle), but its structured result was still very large. It repeated the complete frame ID list in multiple nested range/epoch structures and included full generator parameters, capture-state snapshots, effective policy, measurements, requested query, and provenance despite explicit omission of inline images, snapshot, and page state. Revisit the default temporal bundle projection so ordinary orientation is a genuinely compact resource-and-provenance index, with full manifests and expanded diagnostic/effective structures available only through explicit drill-down.
