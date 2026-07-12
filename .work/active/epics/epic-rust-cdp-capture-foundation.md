---
id: epic-rust-cdp-capture-foundation
kind: epic
stage: drafting
tags: [browser, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
---

# Rust CDP Capture Foundation

## Brief

This epic delivers a trustworthy Rust foundation that launches or attaches to Chrome and continuously receives timestamped visual frames through CDP. It establishes the workspace, domain contracts, Chrome lifecycle, flat target sessions, screencast acknowledgement, normalized session timing, bounded ingestion, and explicit capture-gap reporting that every browser capability relies on.

The work proves the riskiest technical assumption before broader investment: the selected Rust CDP path can sustain real browser capture with sufficient fidelity and expose the raw commands and events Krometrail needs. Compatibility and capture behavior are measured against real Chrome rather than inferred from library APIs.

This epic does not deliver durable history, complete browser automation, temporal artifacts, or agent-facing debugging bundles. It supplies the validated live frame stream and contracts those capabilities consume.

## Foundation references

- `docs/VISION.md` — Local-First Operation and Success
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Rust Workspace, Browser Connection, Target Lifecycle, Frame Ingestion, and Capture Tasks
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity

## Design decisions

- **Rust CDP client selection:** Start with a gated `cdpkit` spike covering every required domain, flat target sessions, raw command/event access, and sustained screencast acknowledgement. Adopt it only if the real-browser compatibility and capture gates pass; otherwise choose between `chromey` and a minimal owned transport from the spike evidence.
- **Legacy runtime removal:** Remove the TypeScript/DAP implementation while establishing the Rust workspace rather than keeping two buildable runtimes. Git tag `v0.2.20` remains the implementation reference if the spike requires recovering prior browser lifecycle or framework-state behavior.

## Anticipated child features

- Rust workspace and core capture contracts
- Rust CDP client research and required-domain compatibility spike
- Chrome discovery, isolated profiles, launch, attach, and shutdown
- Flat target-session supervision and reconnect behavior
- Sustained screencast ingestion, acknowledgement, clocks, and gap statistics
- Real-browser capture-fidelity smoke fixtures

<!-- The design pass on each child feature will fill in real specifics. -->
