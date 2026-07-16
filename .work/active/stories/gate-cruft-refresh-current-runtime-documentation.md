---
id: gate-cruft-refresh-current-runtime-documentation
kind: story
stage: done
tags: [cleanup, documentation, agent-ux]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: cruft
created: 2026-07-15
updated: 2026-07-15
---

# Refresh stale current-runtime documentation for 1.0

## Confidence
Medium

## Category
Stale documentation

## Location
`README.md:3`

## Evidence

README and public guide/reference pages still say browser transport, persistence, MCP tools/resources, and capture configuration are unavailable. The current Rust runtime exposes `doctor` and `mcp`, lifecycle/control/temporal tools, resources, durable recording, and `every_nth_frame` on start/attach requests.

## Removal

Update README, project agent current-state guidance, docs index/guide/reference pages, and `docs/public/llms.txt` to the actual 1.0 command and MCP surface. Regenerate `docs/public/llms-full.txt` through `bun run docs:build`; do not hand-edit it. Preserve intended future-state wording in foundation documents and do not invent CLI commands beyond `src/cli.rs`.

## Implementation notes
- Execution capability: inline direct-read documentation wave; the four release-bound findings share terminology and generated-output verification.
- Review weight: standard; caller explicitly requested all standalone stories remain at `stage: review` for independent bounded review.
- Files changed: `README.md`, `.agents/AGENTS.md` (root `AGENTS.md` symlink), `docs/index.md`, `docs/guide/mcp-configuration.md`, `docs/reference/runtime.md`, `docs/reference/configuration.md`, `docs/public/llms.txt`, and generated `docs/public/llms-full.txt`.
- Tests added/removed: none; this is a current-surface documentation correction.
- Simplification: removed stale unavailable/reserved-runtime prose and kept command/tool detail in the runtime/MCP references rather than preserving duplicate placeholder guidance.
- Discrepancies from design: none.
- Verification evidence: `bun run docs:build` passed (VitePress render/link build, 14 source files); `cargo fmt --all -- --check`, workspace check/test/clippy, and the cross-platform smoke schema suite passed under Rust 1.95.0 (project MSRV 1.85). The generated full file was regenerated after Rust verification because the temporal-evaluation contract test intentionally requires it to be unchanged while tests run.
- Adjacent issues parked: none.


## Review decision

**Approved.** Independent GPT-5.5 standard bounded review found no material blocker. Documentation build, generated-output stability, Rust 1.85 workspace gates, doc tests, and focused MCP/CDP checks pass. The one workspace-count advisory was corrected before completion; no re-review was required.
