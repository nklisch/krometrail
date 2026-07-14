---
id: epic-prove-temporal-advantage-platform-evidence-collection
kind: feature
stage: drafting
tags: [testing, browser, infra]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Platform Evidence Collection

## Brief

Collect and qualify the live evaluation matrix on the supported environments named by the foundation: Linux with stable Chrome and macOS with stable Chrome, including macOS default-DPI and high-DPI configurations. Each platform/configuration result records the exact browser and protocol identity, operating system and architecture, viewport and scale, Rust/toolchain and Git revision, fixture digest, capture configuration, seed, thresholds, and non-claims. Linux Chromium remains a separately labeled best-effort configuration; its absence is not evidence for Chrome or for cross-platform support.

This feature owns platform comparison and evidence publication policy, not a new benchmark or a new browser adapter. It consumes the live qualification harness and keeps observed result files, frames, artifacts, transcripts, and aggregate reports in ignored per-run output storage. A missing macOS lane, failed high-DPI observation, unsupported platform, or unavailable installation remains blocked, skipped, or inconclusive according to the shared evidence state rules; the feature must never write a passing placeholder.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: platform qualification — turns per-platform live runs into bounded, comparable evidence without widening claims beyond tested configurations
- Depends on: `epic-prove-temporal-advantage-live-capture-and-system-qualification`

## Execution boundary

- Opt-in local/hosted collection only; no implicit CI browser download, remote browser fallback, paid agent call, or cross-platform claim from one host.
- Existing committed CDP transport and capture-smoke evidence remains prerequisite context with its own schema and non-claims. This feature's duration, defect, storage, and thesis results must use its own versioned benchmark identity.

## Simplification opportunity

- Reuse the existing per-platform configuration registry, forced-DPI wrapper, canonical evidence serialization, schema validation, cleanup checks, and explicit skip policy. Do not merge transport qualification, capture smoke, and product-thesis evidence into one schema or preserve obsolete evidence formats for unpublished consumers.

## Foundation references

- `docs/SPEC.md` — Supported Environment and Continuous Visual Capture
- `docs/ARCHITECTURE.md` — Browser Connection, Configuration, and Observability
- `docs/EVALUATION.md` — Cross-Platform Evaluation, Capture-Fidelity Evaluation, and Reproducibility
- `docs/evidence/cdp-transport/v2/README.md` — transport evidence boundary
- `docs/evidence/cross-platform-smoke/v1/README.md` — platform smoke boundary and absent high-DPI evidence

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
