---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-docs-skills-alignment
kind: story
stage: implementing
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

- [ ] Current docs alone let a contributor build, test, lint, run, and release Rust.
- [ ] No current instruction or skill claims Bun/TypeScript product runtime, DAP support, npm publication, or unavailable command behavior.
- [ ] Inbound links are repaired and foundation documents remain authoritative.
- [ ] Agile-workflow rules/substrate instructions remain intact.
- [ ] Documentation/link/stale-reference checks pass.
