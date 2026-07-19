---
id: runtime-observation-hardening-capture-acknowledgements
kind: story
stage: implementing
tags: [browser]
parent: runtime-observation-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Keep capture acknowledgements healthy during frame-heavy navigation

Align the production acknowledgement deadline with the qualified transport maximum, preserve immediate one-shot acknowledgement before bounded handoff, and expose privacy-bounded failure reason, deadline, elapsed time, and pipeline counters. Deterministic and real-Chrome evidence must cover the observed frame-heavy failure shape.
