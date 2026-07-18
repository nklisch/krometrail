---
id: epic-browser-interface-hardening
kind: epic
stage: review
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

## Design

Deliver the hardening work through three independently verifiable feature arcs. The arcs share the public-site qualification pass but do not share mutable implementation state, so they can be implemented in any order and integrated through the existing workspace gates.

### Feature 1: Economical agent projections

**Item**: `epic-browser-interface-hardening-economical-projections`

Tighten the MCP-owned compact projections for automatic live snapshots and temporal bundles. Acquisition, canonical resources, and explicit full detail remain unchanged. The compact surface gets one explicit budget and one summary vocabulary so default calls remain useful without repeating provenance.

### Feature 2: Truthful page-context semantics

**Item**: `epic-browser-interface-hardening-page-context-semantics`

Repair semantic document selection, bounded rendered-text scoping, asset identity reconciliation, and focus-policy-aware hidden-target recovery. These changes belong in the existing CDP control authority because it owns document fingerprints, semantic metadata, resource timing interpretation, and interaction preparation.

### Feature 3: Reliable viewport and capture state

**Item**: `epic-browser-interface-hardening-runtime-reliability`

Make viewport verification distinguish declared emulation dimensions from scrollbar-reduced visual content and prevent a transient geometry refresh from terminally failing screencast ingestion. The target-scoped viewport override and capture geometry fence remain the single authorities.

## Dependency graph

The three features have no implementation dependencies. Each preserves current public schemas and can be verified independently; the epic review performs the cross-feature real-Chrome qualification.

## Design decisions

- **Compatibility boundary**: preserve canonical/full MCP results, resource URIs, retained artifacts, and viewport request schemas. Only default compact presentation and erroneous runtime behavior change.
- **Semantic scope**: extend the existing query request with a bounded container relationship only if the current public query model cannot express it; do not infer arbitrary nearby text across the page.
- **Viewport truth**: verify desktop overrides against layout/emulation geometry and report visual content geometry separately, because scrollbars legitimately reduce `cssVisualViewport`.
- **Capture failure isolation**: a failed geometry refresh keeps frames fenced and recorded as bounded gaps until an authoritative refresh succeeds; it does not invalidate prior frames or terminate the target capture pipeline.
- **Qualification**: deterministic protocol regressions are required for every finding; real Chrome/public pages confirm the two browser-dependent runtime repairs and nested-frame semantics.

## Simplification arcs

- Consolidate compact-result budgets and remove projection decisions tied to unrelated response fields.
- Reuse one document-selection path for AX and DOM semantic metadata rather than special-casing frame queries after capture.
- Reconcile asset kind once from initiator and URL identity instead of downstream display corrections.
- Keep viewport acknowledgement and capture geometry transitions in one lifecycle, with an explicit recoverable-gap outcome.

## Risks

- Frame-scoped DOMSnapshot responses include multiple documents and protocol string-table indirection; selecting the wrong document can silently produce `no_match`. Tests must prove the AX and semantic metadata use the same fingerprint.
- Desktop visual viewport dimensions vary with scrollbar state. Relaxing verification too broadly could accept a failed override, so layout/emulation dimensions and device/touch state remain exact authorities.
- Resuming after a geometry-refresh failure can mis-size frames if Chrome changed dimensions. The recovery must retain the fence, declare a gap, and require a later successful refresh before adopting new geometry.

## UI alignment

No product UI surface is introduced. These are MCP/CDP contract repairs and agent-facing response changes, so UI mockups are not applicable.

## Source findings

- `idea-bound-compact-snapshot`
- `idea-compact-temporal-bundle`
- `idea-associate-unnamed-controls`
- `idea-fix-hidden-target-recovery`
- `idea-fix-asset-kind-classification`
- `idea-fix-viewport-preset-regression`
- `idea-fix-frame-envelope-capture`
- `idea-fix-nested-frame-query`

## Child features reviewed and complete

- `epic-browser-interface-hardening-economical-projections` — done after one standard fresh-context pass with no findings.
- `epic-browser-interface-hardening-page-context-semantics` — done after the standard pass's local-container blocker was fixed and the named fix set verified without a second review.
- `epic-browser-interface-hardening-runtime-reliability` — done after the standard pass's geometry-fence and viewport-guidance blockers were fixed and the named fix set verified without a second review.

Aggregate verification is green across workspace tests, workspace check, workspace clippy with warnings denied, formatting, plugin distribution contracts, and opt-in real-Chrome viewport and same-origin-frame qualification.

The aggregate review's named qualification gaps are closed. The exact scrollbar trigger remains locked by the deterministic decoder case (`cssVisualViewport` 375 wide with an exact 390-wide declared/layout viewport). A post-fix run of the current local binary against the original public reproduction, `https://nklisch.github.io/krometrail/` (redirecting to `https://krometrail.dev/`) on managed Chrome 150, applied `responsive_small` successfully with declared/layout/visual 390×844, produced a 390×844 screenshot, and returned no warnings. The real-Chrome browser-context test now installs an actual recording sink and proves persistence advances while capture remains `capturing` with no failure stage after same-origin child-frame navigation; its frame query, stale-reference, and bounded-asset assertions remain in the same flow.

## Review findings (2026-07-18)

**Effective weight**: standard — one same-harness fresh-context aggregate pass.

**Closure policy**: request changes; later closure verifies only this named fix set and does not run another independent epic review.

- **Trigger-specific real-Chrome qualification**: existing opt-in viewport and frame tests passed but did not force scrollbar-reduced visual geometry or assert capture health/persistence across nested-frame navigation. Extend the real-Chrome cases or record equivalent concrete post-fix outcomes before claiming those runtime acceptance points.
- **Stale aggregate decision**: replace the pre-correction statement that refresh failure resumes on last-established geometry with the implemented rule that frames remain fenced/gapped until authoritative refresh succeeds.

Both named findings are fixed and verified. Closure requires no second independent epic review under the recorded standard-weight policy.
