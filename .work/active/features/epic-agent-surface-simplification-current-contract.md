---
id: epic-agent-surface-simplification-current-contract
kind: feature
stage: drafting
tags: [infra, storage]
parent: epic-agent-surface-simplification
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# One current Krometrail contract

## Brief

Remove runtime machinery that exists only to preserve unsupported historical Krometrail releases or hypothetical crate consumers. Keep one current store schema that opens current-format data and rejects older incompatible data before mutation with a clear recovery action. Remove old installer cutoffs, compatibility aliases, default port implementations retained for source compatibility, and contradictory policy/test prose.

This feature does not remove Chrome/CDP compatibility probing, deterministic stable names, evidence algorithm versions, visual-epoch compatibility, or integrity/version checks for the current format. Those are current correctness and provenance mechanisms rather than integration shims.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: contract foundation; response simplification depends on this direction

## Simplification opportunity

Collapse the ordered historical SQL migration chain into current-schema bootstrap plus exact current-version validation; delete unsupported installer upgrade branches/tests, type aliases, const constructors, and trait defaults whose comments identify source compatibility as their sole purpose.

## Foundation references

- `.agents/AGENTS.md` — Current Contract Discipline
- `docs/SPEC.md` — current executable and retained-data contracts
- `docs/ARCHITECTURE.md` — recording store and generated MCP boundary
