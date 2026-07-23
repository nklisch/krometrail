---
id: idea-cdpkit-byte-fingerprint-hook
created: 2026-07-23
updated: 2026-07-23
tags: [perf]
---

The designed-but-deferred opt-4 of `feature-perf-wait-snapshot-pipeline`:
reach the ~1–5 ms quiescent semantic-wait poll by reusing the previously
decoded `ActiveSnapshot` when the raw AX/DOM response bytes are unchanged.
Blocked on the transport seam: cdpkit 0.4.0's read loop (inner.rs:230-233)
unconditionally parses every inbound frame to `serde_json::Value` for id
routing and discards the original bytes, so no pre-parse fingerprint point
exists from Krometrail's side. Options when picked up: upstream a
byte-level response hook to cdpkit, maintain a fork, or vendor the seam.
The full design (fingerprint keys, reuse conditions, invalidation rules,
pre-mortem: fingerprint collision, identical-content reload, DOM/AX
divergence, capability widening, generation churn) is already written in
the feature body's opt-4 unit and Risks section — reuse it. Parse cost
this would eliminate on quiescent polls: ~66 ms at 50k nodes, plus decode
reuse. Opts 1–3 (implemented) already brought miss polls to ~86–100 ms.
