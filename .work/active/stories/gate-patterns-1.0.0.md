---
id: gate-patterns-1.0.0
kind: story
stage: done
tags: [patterns]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: patterns
created: 2026-07-15
updated: 2026-07-15
---

# Patterns extracted for 1.0.0

## New patterns codified

- `validated-wire-contracts` — wire decoding delegates to domain validation and source-aligned schemas.
- `injected-core-ports` — domain ports flow inward and concrete adapters are root-wired.
- `registry-declared-surfaces` — one registry drives variant identities, metadata, schemas, and routes.
- `bounded-loss-accounting` — bounded streams surface rejected/missed observations explicitly.
- `single-writer-effect-reducer` — lifecycle inputs reduce to deterministic state and explicit effects.
- `layered-cdp-qualification` — scripted, fault-proxy, and opt-in Chrome tests form one ladder.
- `ordered-sql-migrations` — immutable contiguous SQL revisions apply transactionally.
- `canonical-json-schema-artifacts` — Rust contracts generate canonical checked-in JSON/schema artifacts.

## Inconsistencies flagged

None.

## Pattern files written

- `.agents/skills/patterns/*.md`
- `.agents/skills/patterns/SKILL.md`
- `.agents/rules/patterns.md`
- `.claude/skills/patterns` compatibility symlink
