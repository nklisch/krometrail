---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate
kind: feature
stage: drafting
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# CDP Transport Compatibility Gate

## Brief

Prove the selected Rust CDP path against real Chrome before production lifecycle and capture code commits to it. A deliberately disposable `cdpkit` spike exercises every required protocol domain, browser-level commands, flat target sessions, typed operations, raw command and event escape hatches, and sustained `Page.screencastFrame` acknowledgement while recording browser and protocol versions as evidence.

Turn the spike results into an explicit transport decision: adopt `cdpkit` only when all required gates pass; otherwise use the evidence to choose between `chromey` and a minimal owned transport. Spike-only scaffolding must not become the production capture pipeline. This feature qualifies and selects the adapter mechanism; it does not own Chrome profiles, reconnect supervision, or bounded production ingestion.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: transport gate — depends on the Rust contracts and blocks production browser integration
- Design decisions inherited: evidence-gated `cdpkit` adoption with an explicit `chromey` or owned-transport fallback

## Foundation references

- `docs/SPEC.md` — Supported Environment, Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Browser Connection, Frame Ingestion, and Technology Decisions
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity

## Research grounding

- Versioned evidence: `docs/research/rust-cdp-transport-2026-07.md`
- Execution reference: `.agents/skills/rust-cdp-transport/SKILL.md`
- Dispatch rationale: direct-read only because the autopilot caller prohibited subagents and questions; crates.io metadata, pinned published source, CI/tests, current GitHub issues, and the official CDP source supplied the required evidence.
- Decision posture: no transport is selected from documentation or source inspection. Spike exact `cdpkit` 0.4.0 first against real Chrome, then apply the documented pass/fallback rules unchanged. Test `chromey` 2.52.0 only for a demonstrated lifecycle, ordering, or sustained-capture failure that its handler could solve; use a minimal owned raw-envelope transport when either library loses protocol evolution before the raw boundary, obscures prompt acknowledgement/backpressure, misroutes sessions, or requires a fork.
