---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
kind: feature
stage: drafting
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Bounded Screencast Ingestion

## Brief

Turn each supervised page target into a production live visual stream without allowing storage or later image work to stall CDP. Accept compressed screencast frames into a bounded ingestion path, acknowledge promptly after the acceptance decision, preserve sequence and viewport metadata, and expose per-target statistics for received, accepted, and dropped frames.

Normalize every observation onto a monotonic session clock while preserving Chrome source time and daemon observed time as distinct evidence. Saturation, sequence loss, target visibility pauses, and other capture interruptions produce explicit, differently classified gaps rather than implied continuity. Cancellation stops acceptance, drains or reports accepted work under a bounded flush policy, and leaves downstream persistence behind a port; this feature does not implement durable segments, retention, or temporal artifacts.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: core capture capability — consumes supervised flat target sessions and supplies the validated live frame stream to later storage work
- Design decisions inherited: the production path uses the transport selected by the real-Chrome gate; spike code remains separate

## Foundation references

- `docs/SPEC.md` — Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Time Model, Frame Ingestion, Capture Tasks, Failure Isolation, and Observability
- `docs/VISUAL-EVIDENCE.md` — Source Frames and Capture Gaps
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity
