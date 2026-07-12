---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-docs-skills-alignment
kind: story
stage: review
tags: [infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Align contributor docs and skills with the Rust runtime

## Scope

Implement the parent feature's Unit 7. Roll README, agent instructions, documentation navigation, and skill catalogs forward to one truthful Rust runtime. Remove current-tree legacy/generated product docs and published skills that advertise deleted DAP, TypeScript, framework-observation, Chrome, or MCP command surfaces.

Do not invent replacement command guidance before those capabilities exist. Preserve the agile-workflow substrate/rules and the five authoritative foundation documents.

## Implementation requirements

- Document current Rust workspace commands, structure, supported environment, limited executable state, and release process.
- Remove migration-history prose; Git and remote `v0.2.20` carry history.
- Delete obsolete citty, DAP, framework-devtools, TypeScript refactor, and published Krometrail command skills after checking inbound links.
- Reduce/update `tap.json`, plugin settings, and mirrors so no unavailable command is advertised.
- Delete `docs/legacy/` and old generated runtime docs after tag verification; keep foundation navigation current.
- Change foundation docs only for a verified contradiction, not to preserve legacy notes.

## Acceptance criteria

- [x] Current docs alone let a contributor build, test, lint, run, and release Rust.
- [x] No current instruction or skill claims Bun/TypeScript product runtime, DAP support, npm publication, or unavailable command behavior.
- [x] Inbound links are repaired and foundation documents remain authoritative.
- [x] Agile-workflow rules/substrate instructions remain intact.
- [x] Documentation/link/stale-reference checks pass.

## Implementation notes

- Preserved the interrupted deletion set after verifying the working tree and diff. Removed the obsolete docs trees (`docs/legacy/`, old generated/runtime pages, old language pages, and stale landing components), the browser command skill, published Krometrail skills, citty/framework-devtools/TypeScript refactor skills, and their Claude mirrors. The five foundation documents and `.work` substrate remain.
- Rewrote `README.md`, `.agents/AGENTS.md` (the `AGENTS.md` and `CLAUDE.md` symlinks therefore resolve to the same content), `docs/agents.md`, the VitePress index/nav, current development/MCP/runtime/configuration/privacy pages, generated LLM docs, and the Open Graph image. The docs now distinguish intended Chrome/Electron-renderer contracts from the current Rust binary and keep Bun limited to documentation/fixture tooling.
- Reduced catalogs and plugin configuration to no installed Krometrail skills and no MCP permission/server advertisement. Removed the stale pattern mirror and settings that described deleted TypeScript/DAP behavior. Preserved `.agents/rules/agile-workflow.md`.
- Kept all classified browser fixtures and updated their stale observer-only comments so React/Vue fixture names describe target applications, not product framework-state support. No foundation document was changed.
- Verification passed: `git ls-remote --tags origin refs/tags/v0.2.20` returned `3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558`; Rust fmt/check/test/clippy passed (29 tests); shell syntax and `tests/distribution-static.sh` passed; `bun run docs:build` passed with zero dead-link warnings; `git diff --check` passed; stale path/command scans found no current docs, skills, plugin, or runtime claims for removed commands. `.pi/` was not touched or staged.
- Files intentionally excluded from cleanup: the five authoritative foundation docs, `.work`, agile-workflow rules, classified browser fixtures, and current distribution implementation except factual documentation/link updates.
