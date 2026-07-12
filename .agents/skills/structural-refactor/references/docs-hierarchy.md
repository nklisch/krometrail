# Rule: Docs Hierarchy

> Foundation docs are the authoritative source of truth. Historical implementation material lives in Git at `v0.2.20`; `legacy/` is deprecated and `.generated/` is never edited directly.

## Motivation

As a project evolves, documentation drifts. Without a clear hierarchy, agents and contributors
read outdated design docs and make decisions based on stale information. Explicit tiers
prevent this: foundation docs are maintained and trusted, everything else has a clear label
indicating its reliability.

## Before / After

### From this codebase: current docs layout

**Good — explicitly tiered:**
```
docs/
├── VISION.md                  [AUTHORITATIVE] Product purpose and boundaries
├── SPEC.md                    [AUTHORITATIVE] Behavioral contracts
├── ARCHITECTURE.md            [AUTHORITATIVE] System structure
├── VISUAL-EVIDENCE.md         [AUTHORITATIVE] Temporal visual language
├── EVALUATION.md              [AUTHORITATIVE] Validation gates
├── agents.md                  [NAVIGATION] Which docs to trust
├── .generated/                [LEGACY GENERATED] v0.2.20 reference only
└── legacy/                    [DEPRECATED] Replaced product documentation

Git tag v0.2.20                [HISTORICAL] Prior designs and implementation
```

### Synthetic example: undifferentiated docs

**Before:**
```
docs/
  architecture.md           (current? or from 6 months ago?)
  api-design.md             (implemented? or proposal?)
  old-api-design.md         (clearly old but still here)
  generated-schema.md       (hand-edited copy of generated file)
```

**After:**
```
docs/
  architecture.md           [AUTHORITATIVE — kept current]
  legacy/
    old-api-design.md       [DEPRECATED — clearly labeled]
  .generated/
    schema.md               [AUTO-GENERATED — never edit]

git history
  api-design.md             [HISTORICAL — recover by revision]
```

## Exceptions

- Active designs live in their `.work/` item bodies rather than standalone design documents.
- **agents.md** is a meta-document that describes the hierarchy itself — it is authoritative.

## Scope

- Applies to: all files under `docs/`
- Does NOT apply to: code comments, inline documentation, README.md
