---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-architecture-final5
kind: story
stage: implementing
tags: [browser, prose]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-event-stream-closure]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Remove stale final5 qualification contradiction

## Origin

Adversarial feature review found `docs/ARCHITECTURE.md` simultaneously names current final5 evidence and claims the historical schema-v2 run still needs canonical-trace requalification.

## Scope

Replace the stale Technology Decisions assertion in place so it names the current final5 exact cdpkit 0.4.0 decision and retains the runtime compatibility-probe and explicit limitation requirements. Cross-check Browser Connection and current evidence docs for contradictions. Regenerate documentation; do not add historical/migration prose.

## Acceptance criteria

- [ ] Architecture consistently names final5 as current decisive evidence and retains replaceability/runtime probe/limitations.
- [ ] No stale “until requalified” assertion remains in foundation docs.
- [ ] Docs build passes and generated output is regenerated, not hand-edited.
