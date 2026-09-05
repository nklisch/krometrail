---
id: epic-a-grade-reliability-page-selection-recovery
kind: feature
stage: backlog
tags: [browser, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Diagnose and make page-selection failures recoverable

## Outcome and priority

An agent could see a running browser on the desktop but could not address its page. Its first screenshot omitted required arguments (agent error); the corrected call failed selection, and success-only page listing prevented recovery. Do not treat desktop visibility as proof that CDP had attached a selectable page.

- **Priority:** P1 — wave 1 of [epic-a-grade-reliability](epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** User-reported incident; selection bug versus attachment timing/context versus integration loss remains unresolved.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Backlog scope and acceptance criteria, not an approved implementation design. Scope/design before delivery; no implementation or paid qualification is authorized by capture alone.

## Evidence

- Recent local report: corrected screenshot returned "selected browser page was not found" after successful launch/navigation
- crates/krometrail-cdp/src/targets/ — selection/attachment lifecycle
- crates/krometrail-mcp/src/session.rs — session ownership
- crates/krometrail-mcp/src/registry.rs — tool dispatch and recovery surface

## Acceptance criteria

- [ ] Reproduce on a local fixture with server/client versions and correlate launch, attachment, selected target, screenshot request, list response, and client-visible data.
- [ ] Distinguish invalid/missing request arguments, no attached page yet, stale/closed selection, wrong session/target, and lost response details; record the established cause or the remaining evidence gap.
- [ ] The normal single-page startup journey reaches an explicitly addressable page. Timing races use a bounded readiness/retry contract rather than arbitrary sleeps.
- [ ] Missing/stale selection provides an executable recovery path with available target identities or a precise wait/list instruction that actually exposes them.
- [ ] Multi-page ambiguity never silently selects the wrong page. Cover startup, tab close/replacement, reconnect, popup, and explicit target selection.
- [ ] A code defect is fixed only after demonstrated; if this incident is entirely result-delivery loss, retain the reproduction/trace and close this investigation without duplicating the delivery fix.

## Implementation direction and boundaries

Preserve explicit page authority while making common-case defaults and recovery ergonomic. Coordinate with result delivery but allow investigation to proceed in parallel.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-temporal-range-active-target-defaults` — related authority/context, not an implicit blocking dependency.

## Live usage after user update/restart

The user requested a real example-site exercise after updating Pi Extensions and restarting. Two fresh managed temporary sessions reproduced an attachment/selection failure through the installed Krometrail server, which reported `server_version: 1.6.2`. Browser build and exact binary commit were not captured; this is installed-runtime evidence, not a claim about every client or platform.

- `start_browser {profile:"temporary",focus:"preserve",initial_url:"https://example.com"}` succeeded. Scripted extraction of decoded `list_pages` returned an empty array; status reported ready, page_count 0, selected_target_id null, capture empty, and no retained bounds.
- A correctly shaped viewport PNG request failed `not_found: selected browser page was not found`. `create_page` for the same site returned degraded live evidence; its subsequent inventory contained one selected hidden target, id `a7e706c2-bf63-4a4e-9789-b14800f26300`, attachment_generation 1. `activate_page {}` also reported incomplete live evidence. The default screenshot still failed `not_found`; explicitly passing that returned target id failed `target_failed: browser page is not currently attached`.
- After stopping that session, a fresh foreground session at `https://example.org` again reported ready with zero pages, no selection, and empty capture. `create_page` then failed `target_failed: created browser target could not be attached`; a subsequent inventory remained empty.
- Both owned sessions were stopped successfully. No account, form submission, clipboard, retained artifact generation, desktop fallback, oversized page, or browser mutation outside these test sessions was used. No screenshot, interaction, or capture qualification passed.

A separate installed-adapter defect is also still present: ordinary gateway listing exposed only `list_pages succeeded`, while the scripting path exposed the canonical empty or populated inventory. The actual installed adapter's source and compiled resolver both still contain the early return for nonempty content. Thus missing model-visible facts remain a delivery problem, but the decoded zero-page status and explicit target-attachment failures demonstrate a separate runtime problem. This does not establish the root cause or timing of attachment failure; preserve that distinction during diagnosis.
