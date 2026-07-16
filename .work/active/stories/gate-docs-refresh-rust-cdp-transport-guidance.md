---
id: gate-docs-refresh-rust-cdp-transport-guidance
kind: story
stage: done
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

## Implementation notes
- Execution capability: inline direct-read documentation wave; production connector/session/capture sources were checked before updating the skill and research reference.
- Review weight: standard; caller explicitly requested all standalone stories remain at `stage: review` for independent bounded review.
- Files changed: `.agents/skills/rust-cdp-transport/SKILL.md`, `docs/research/rust-cdp-transport-2026-07.md`, and generated `docs/public/llms-full.txt`.
- Tests added/removed: none; the existing transport and capture tests remain the executable contract.
- Simplification: replaced “production work remains later” wording while retaining the exact qualification-spike limits around named-event scope, unbounded subscriber queues, and reconnect/session ownership.
- Discrepancies from design: none.
- Verification evidence: `bun run docs:build` passed after the research edit; `cargo fmt --all -- --check`, workspace check/test/clippy, and the cross-platform smoke schema suite passed under Rust 1.95.0 (project MSRV 1.85).
- Adjacent issues parked: none.


## Review decision

**Approved.** Independent GPT-5.5 standard bounded review found no material blocker. Documentation build, generated-output stability, Rust 1.85 workspace gates, doc tests, and focused MCP/CDP checks pass. The one workspace-count advisory was corrected before completion; no re-review was required.
