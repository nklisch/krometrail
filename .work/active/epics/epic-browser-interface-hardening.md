---
id: epic-browser-interface-hardening
kind: epic
stage: drafting
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Browser Interface Hardening

## Brief

Repair the eight regressions and agent-friction findings verified by the post-1.1.0 cross-surface manual pass. Krometrail must remain economical by default, retain exact full-evidence drill-down, correctly model page and frame context, and keep controlled capture and viewport state reliable on real public websites.

The verified scope is:

- bound default live snapshots so routine mutations do not dominate agent context;
- make the default temporal bundle a genuinely compact index rather than repeated full provenance;
- allow a caller to associate unnamed controls with a bounded rendered-text container;
- make hidden-target recovery truthful under `focus: preserve`;
- classify page assets consistently with observable resource identity;
- restore responsive viewport presets on real managed Chrome;
- prevent nested-frame pages from terminating screencast capture at `frame_envelope`;
- make qualified same-origin nested-frame semantic queries inspect the referenced document.

## Strategic decisions

- Preserve all existing 1.x canonical/full result and resource contracts; compact projections may remove repeated/default detail only from the default agent presentation.
- Fix frame and viewport behavior inside the existing target-scoped CDP authority rather than adding alternate automation paths or compatibility shims.
- Treat the recorded public-site reproductions as qualification cases, backed by deterministic regression tests and bounded real-Chrome confirmation where the failure depends on Chrome behavior.

## Simplification opportunity

Centralize compact-result budgets and resource-kind reconciliation instead of layering special cases at individual tool routes. Reuse one document-resolution path for main-document and qualified-frame queries, and one acknowledged viewport lifecycle path for presets and custom metrics.

## Anticipated child features

- economical default projections for live and temporal results;
- page-context semantics for rendered-text scope, frames, assets, and hidden-target recovery;
- CDP runtime reliability for viewport overrides and screencast frame ingestion.

## Source findings

- `idea-bound-compact-snapshot`
- `idea-compact-temporal-bundle`
- `idea-associate-unnamed-controls`
- `idea-fix-hidden-target-recovery`
- `idea-fix-asset-kind-classification`
- `idea-fix-viewport-preset-regression`
- `idea-fix-frame-envelope-capture`
- `idea-fix-nested-frame-query`
