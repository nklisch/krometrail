---
id: gate-docs-refresh-rust-cdp-transport-guidance
kind: story
stage: implementing
tags: [documentation, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: docs
created: 2026-07-15
updated: 2026-07-15
---

# Refresh Rust CDP transport skill and research guidance

## Drift category
Stale skill/research reference

## Location
- Docs: `.agents/skills/rust-cdp-transport/SKILL.md:13`; `docs/research/rust-cdp-transport-2026-07.md:7,101`
- Contradicting sources: production connector/session/capture pipeline and root composition in `crates/krometrail-cdp` and `src/app.rs`

## Current doc text

> Production lifecycle and capture implementation remain later work.

## Contradiction

Production browser connection, capture ingestion, reconnect supervision, recording storage, and root composition are implemented. The qualification spike remains non-default, but it is no longer the only transport/capture implementation.

## Required edit

Update the skill and research reference in place to current implementation status while preserving exact cdpkit/API limitations and the distinction between production runtime and qualification spike. Regenerate public documentation with `bun run docs:build` after editing the research page.
