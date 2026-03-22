# Rule: Docs Hierarchy

> Foundation docs are authoritative source of truth. designs/completed/ is historical context. legacy/ is deprecated. .generated/ is auto-generated (never edit).

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
├── ARCH.md                    [AUTHORITATIVE] System architecture
├── SPEC.md                    [AUTHORITATIVE] Adapter contract, limits
├── UX.md                      [AUTHORITATIVE] Viewport interaction design
├── VISION.md                  [AUTHORITATIVE] Problem statement, scope
├── ADAPTER-SDK.md             [AUTHORITATIVE] How to write adapters
├── PRIOR_ART.md               [AUTHORITATIVE] Competitive analysis
├── agents.md                  [NAVIGATION] Which docs to trust
├── .generated/                [AUTO-GENERATED] From Zod schemas — never edit
├── designs/completed/         [HISTORICAL] Phase design docs — all implemented
├── legacy/                    [DEPRECATED] ROADMAP, INTERFACE, TESTING
├── browser/                   [DOMAIN REFERENCE] Browser subsystem
├── framework-state/           [DOMAIN REFERENCE] Per-framework specs
└── languages/                 [DOMAIN REFERENCE] Per-language adapter docs
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
  designs/completed/
    api-design.md           [HISTORICAL — implemented, context only]
  .generated/
    schema.md               [AUTO-GENERATED — never edit]
```

## Exceptions

- **New design docs** being actively developed may live in `docs/designs/` (not `completed/`)
  temporarily. Move to `completed/` once implemented.
- **agents.md** is a meta-document that describes the hierarchy itself — it's authoritative.

## Scope

- Applies to: all files under `docs/`
- Does NOT apply to: code comments, inline documentation, README.md
