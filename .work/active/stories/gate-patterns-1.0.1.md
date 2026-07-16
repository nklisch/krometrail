---
id: gate-patterns-1.0.1
kind: story
stage: done
tags: [patterns]
parent: null
depends_on: []
release_binding: 1.0.1
gate_origin: patterns
created: 2026-07-16
updated: 2026-07-16
---

# Patterns extracted for 1.0.1

## New patterns codified

- `exact-release-managed-activation` — exact package/runtime release coupling, verification, and transactional version projection.
- `hermetic-release-boundary-fixtures` — temporary fake commands/assets exercise distribution seams without network or user-home mutation.

## Inconsistencies flagged

None.

## Pattern files written

- `.agents/skills/patterns/exact-release-managed-activation.md`
- `.agents/skills/patterns/hermetic-release-boundary-fixtures.md`
- `.agents/skills/patterns/SKILL.md` (updated index)
- `.agents/rules/patterns.md` (regenerated hook-loaded digest)
