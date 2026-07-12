---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-architecture-ack-order
kind: story
stage: review
tags: [bug, browser, documentation]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Align foundation architecture with acknowledgement-before-handoff

## Origin

Second adversarial feature review found `docs/ARCHITECTURE.md` prose selecting acknowledgement before bounded handoff while the Frame Ingestion diagram shows enqueue before acknowledgement.

## Scope

Correct the authoritative Frame Ingestion diagram and accompanying prose so frame receipt is followed immediately by CDP acknowledgement, then bounded handoff, then explicit capture-gap recording when enqueue fails. Cross-check SPEC/VISUAL-EVIDENCE and generated docs for contradictions. No runtime behavior change.

## Acceptance criteria

- [x] Foundation diagrams and prose consistently show acknowledgement before bounded handoff.
- [x] Failed handoff records an explicit capture gap after acknowledgement.
- [x] Documentation build passes and no generated file is hand-edited.

## Implementation notes

- Corrected `docs/ARCHITECTURE.md`'s Frame Ingestion diagram and prose to model receive → immediate acknowledgement → bounded handoff → explicit gap on enqueue failure, including post-receive ack-latency semantics.
- Corrected `docs/SPEC.md`, `docs/VISUAL-EVIDENCE.md`, `docs/research/rust-cdp-transport-2026-07.md`, and `.agents/skills/rust-cdp-transport/SKILL.md` to carry the same contract.
- Regenerated `docs/public/llms-full.txt` through `bun run docs:build`; no generated file was hand-edited. No runtime files changed.
- Verification: `bun run docs:build`; contract grep rejects the former enqueue-before-ack wording and confirms the receive/ack/handoff/gap ordering; `git diff --check` passes.
- Restored `.work/bin/work-view`; `.pi/` remains ignored.
