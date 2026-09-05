---
id: epic-a-grade-reliability-agent-result-delivery
kind: feature
stage: drafting
tags: [agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Preserve essential results in the agent-visible MCP response

## Outcome and priority

Text content contains only a success summary while usable result data is separate structured content. A client exposing text alone can hide page IDs and other recovery authorities. A recent local agent report observed exactly a success-only list_pages result, but the server wire response and integration renderer were not captured together.

- **Priority:** P1 — wave 1 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced response behavior; the cause of the reported integration incident is not yet established.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized for the bounded checkpoint/design below after the user asked to continue execution. No release or paid model-effectiveness qualification is authorized.

## Evidence

- crates/krometrail-mcp/src/response.rs:583 — successful text summary
- crates/krometrail-mcp/src/response.rs:989–1041 — text content versus structuredContent
- crates/krometrail-mcp/src/response.rs:1158 — ListPages result projection

## Acceptance criteria

- [ ] Capture a privacy-safe comparison of server wire result, client-decoded result, and model-visible result for list_pages, browser_status, inspect_page, temporal range resolution, and representative failures. Record plugin, binary, client, and protocol versions.
- [ ] Every currently supported integration exposes the essential identifiers, outcomes, recovery guidance, and requested observations needed for the next action; a bare success line is not sufficient for a data-returning tool.
- [ ] Add regression coverage through the actual integration delivery boundary, not only assertions against structuredContent inside Rust.
- [ ] Preserve bounded output, omission reporting, image/resource delivery, and privacy. Do not dump unlimited JSON into text or create compatibility paths for hypothetical clients.

## Implementation direction and boundaries

Fix supported-client delivery or supply a bounded useful text projection according to observed integration behavior. Keep one canonical result authority; the rendering strategy is not settled by this backlog item.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-mcp-locator-ergonomics` — related authority/context, not an implicit blocking dependency.

## Authorized execution checkpoint — 2026-09-05

The user asked to continue the reliability plan after the accepted Flash pilot. This item starts with an Astra-medium investigation/design checkpoint: trace the canonical result through supported integration delivery, identify the concrete consumer that loses essential data, and propose the smallest bounded current-contract correction with a reproducer. Preserve action outcomes, resource/image delivery, privacy, and omission accounting. Do not add compatibility text for hypothetical consumers or implement a broad projection redesign before the delivery boundary is established. Record evidence, affected file ownership, and a focused verification plan here for parent adjudication.
