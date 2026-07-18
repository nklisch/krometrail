---
id: epic-agent-browser-ergonomics
kind: epic
stage: done
tags: [agent-ux, browser, visual]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Agent browser ergonomics

## Brief

Make Krometrail's evidence-rich browser surface as economical and composable for ordinary agent work as contemporary Playwright-backed browser controls, without weakening its stable 1.x evidence, recovery, and provenance contracts. The work follows a live comparison against Codex's isolated in-app browser and Chrome extension: Krometrail led on temporal evidence, responsive-device precision, mutation verification, and structured recovery, but required larger payloads, more snapshot round-trips, verbose resolved-range copying, and more browser-lifecycle knowledge.

Deliver additive response projections and concise status, semantic locator queries with explicit frame scope, reusable temporal range handles, intention-revealing viewport presets and mismatch guidance, and a targeted parity layer for browser state agents routinely need to inspect or control. Reusable named managed profiles already exist; this epic improves their agent-facing discoverability instead of creating a second persistence system.

The resulting stable surface remains local-first and evidence-oriented. Full observations, exact resolved ranges, current custom viewport metrics, canonical resources, and existing request meanings remain available for 1.x clients. Compactness and convenience are new choices layered over those authorities.

## Strategic decisions

- **Compatibility**: preserve explicit full/legacy response expansion and every authoritative evidence/resource contract, while intentionally changing omitted presentation preferences to the lower-cost compact agent default. Record the default change plainly and cover it at the generated-schema and stdio boundaries.
- **Product position**: optimize Krometrail for trustworthy diagnosis and temporal evidence while adding the smallest browser-control conveniences that remove repeated agent friction; do not recreate all of Playwright.
- **Response authority**: projections may omit inline presentation detail but never discard retained evidence or change the authoritative domain result.
- **Browser-state boundary**: add privacy-bounded metadata and explicit user-directed control for clipboard, downloads, frames, popups, and page assets; do not broaden into unrestricted DevTools or response-body capture.
- **Profile continuity**: retain named Krometrail-managed profiles as the supported continuity mechanism and teach agents when to select one; attachment to the user's default browser remains explicit.

## Simplification opportunity

Consolidate response selection in the existing MCP projector, locator resolution in the existing snapshot/reference registry, temporal follow-up inputs behind one range authority, and viewport intent in the existing target-scoped override lifecycle. Reuse target supervision and browser-event infrastructure for popup/download discovery. Avoid parallel schema registries, duplicated range persistence, a second profile manager, or tool-specific compacting logic.

## Design decisions

- **Projection depth**: presentation controls reduce the MCP payload first; they do not suppress the underlying action observation in this minor release because that evidence is part of the mutation contract.
- **Ergonomic default**: omitted response preferences select compact structured detail, omit inline image bytes, and select concise status. Callers expand with explicit full/inline options; an explicit legacy projection remains available for clients that need the former presentation shape.
- **Live image omission**: `omit` removes inline bytes; resource-only mode applies only where a canonical retained resource exists. Live post-action screenshots remain inline-or-omit until a durable live-image authority is independently justified.
- **Semantic action model**: a semantic query returns exact references; existing actions continue consuming references. Automatic action-time locator reevaluation is deferred because it weakens explicit ambiguity and stale-reference behavior.
- **Range-handle lifetime**: handles are opaque, process-local, immutable references to validated resolved ranges. They survive browser stop while retained evidence remains available, but not MCP restart or session deletion; every use revalidates retained availability.
- **Viewport presets**: presets materialize into the existing override and report intent plus effective geometry. They do not emulate user agents.
- **Parity boundary**: implement bounded browser/session metadata and explicit controls, not arbitrary DevTools access or response-body capture.

## Decomposition

Split by public capability so each feature can design and verify one stable contract while sharing existing registries and ports. Four contract features are independent; the browser-state parity feature composes target/frame lifecycle and therefore follows semantic targeting.

### Child features

- `epic-agent-browser-ergonomics-response-projections` — additive response detail selection plus concise browser status — depends on: `[]`
- `epic-agent-browser-ergonomics-semantic-targeting` — bounded main-document semantic query-to-reference matching with descendant scope — depends on: `[]`
- `epic-agent-browser-ergonomics-temporal-range-handles` — process-local opaque handles accepted by temporal follow-up tools — depends on: `[]`
- `epic-agent-browser-ergonomics-viewport-intent` — responsive/mobile presets and effective-layout mismatch guidance — depends on: `[]`
- `epic-agent-browser-ergonomics-browser-contexts` — managed-profile discovery, popup relationships/waits, frame inventory/scoping, and page-asset metadata — depends on: `[epic-agent-browser-ergonomics-semantic-targeting]`
- `epic-agent-browser-ergonomics-local-io` — explicit clipboard and managed-download workflows with canonical local resources — depends on: `[]`

### Simplification arcs

- Response projection and compact status share one MCP detail vocabulary and projector.
- Semantic targeting reuses exact snapshot references rather than creating persistent locator identities.
- Range handles terminate at one application-service lookup authority; storage and artifact ports remain range-based.
- Viewport presets reuse the lifecycle-complete override state instead of adding a device-emulation subsystem.
- Browser contexts reuse target supervision, snapshot identity, sanitized resource metadata, and the existing managed-profile launcher.
- Local I/O reuses explicit operation mutability, browser events, and canonical resources while keeping content out of logs.

### Decomposition risks

- Additive fields still affect generated schemas and every registry-derived batch shape; canonical schema tests are release-critical.
- Clipboard and download behavior depends on browser permissions and platform paths; privacy and ownership must fail closed.
- Same-origin frame targeting must retain target/document generation fences and never blur cross-origin boundaries.
- Frame-scoped interaction is the highest-risk context slice; design must distinguish same-process same-origin support from OOPIF and cross-origin boundaries.
- Clipboard and downloads require explicit managed-versus-attached authority decisions and must keep content and local paths out of diagnostics.

## Release intent

Ship the completed epic as the next minor release after feature and aggregate review. Release binding remains late-bound until implementation and review are complete.

## Implementation handoff

All six child features are complete and have passed one fresh standard review with accepted material findings repaired. The combined surface now defaults to compact structured responses with no inline image bytes, while explicit full, inline, legacy, custom-metrics, frame, asset, context, and local-I/O expansion remains available. Focused validation includes real-Chrome semantic targeting, viewport, and managed-download workflows plus strict core/CDP clippy and registry/schema coverage.

Aggregate review should concentrate on cross-feature schema consistency, default projection behavior, session/reconnect cancellation, evidence privacy, canonical resource lifetime, skill guidance, and foundation-document drift before binding the minor release.

## Aggregate review

Standard review requested changes and was resolved in one pass:

- Managed-download interception moved from browser startup to explicit first `list_downloads` activation, preserving existing managed and named-profile defaults while keeping subscription-before-enable ordering.
- Page-target state now enforces the 128-live-page bound, reclaims terminal state under sustained churn, and preserves monotonic cursors and immutable opener identity.
- Semantic outcome wording, public generated docs, batch projection guidance, managed-download URI guidance, root agent rules, and stdio smoke counts were rolled forward to the implemented contract.
- An adjacent macOS release-test failure was traced to comparing canonical and non-canonical temporary path spellings; the test now compares one canonical path domain without changing production profile behavior.

Review evidence includes 10 lazy-download tests, 13 reducer unit tests, 13 target-reducer integration tests including 10,001-cycle churn, 6 root runtime smoke tests, 160 CDP library tests, docs generation, workspace checks, and warning-denied core/CDP clippy slices. The full release helper remains the final shipment gate.
