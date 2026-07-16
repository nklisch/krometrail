---
id: gate-cruft-refresh-current-runtime-documentation
kind: story
stage: drafting
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
