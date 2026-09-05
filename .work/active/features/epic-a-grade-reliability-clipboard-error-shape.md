---
id: epic-a-grade-reliability-clipboard-error-shape
kind: feature
stage: implementing
tags: [browser, agent-ux, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Classify clipboard failures from the actual CDP result shape

## Outcome and priority

The clipboard classifier searches /result/exceptionDetails even though the unwrapped command result exposes exceptionDetails at its top level. Focus, secure-context, or unavailable-API failures can receive generic permission-denied recovery advice; existing classifier tests use the nested shape.

- **Priority:** P2 — wave 1 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Code-traced extractor/test mismatch; permission automation remains a separate issue.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Selected for a bounded GLM-5.3-Flash xhigh implementation trial after the user requested trying that model. The design below scopes this pilot; the broader backlog and paid/browser qualification are not activated.

## Evidence

- crates/krometrail-cdp/src/control/clipboard.rs:252 — nested-only exception extractor
- crates/krometrail-cdp/src/control/evaluation.rs:58 — top-level exception handling
- crates/krometrail-cdp/src/transport/cdpkit.rs:103 — raw result boundary

## Acceptance criteria

- [ ] Use actual transport-shaped fixtures for focus_required, secure_context_required, clipboard_unavailable, permission denial, malformed results, and success.
- [ ] Each known failure reports its accurate category and actionable recovery; unknown exceptions do not become a confident permission diagnosis.
- [ ] Normalize the supported transport envelope consistently without proliferating legacy result variants.
- [ ] Keep raw clipboard content and sensitive exception material out of diagnostic summaries.

## Implementation direction and boundaries

Correct result normalization and classification before assuming that every observed clipboard failure requires a permission grant.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Related existing work

- `idea-browser-automated-clipboard-permissions` — related authority/context, not an implicit blocking dependency.

## Bounded pilot design — 2026-09-05

The user requested a GLM-5.3-Flash xhigh implementation trial and a candid performance assessment. This item is the selected bounded pilot, not commencement of the whole reliability program.

Current source inspection confirms `CdpkitTransport::send_raw` forwards the unwrapped command result; `Runtime.callFunctionOn` places `exceptionDetails` beside its `result` remote object. The clipboard classifier instead reads `/result/exceptionDetails`, and its unit fixtures repeat that incorrect nesting. Its catch-all also asserts permission denial when the cause is unknown.

Keep the change local to clipboard result decoding and its deterministic transport-shaped tests. Use one current command-result representation, preserve successful reads/writes and byte limits, and ensure an exception cannot be accepted as a successful value. Classify the bridge's known secure-context, focus, and unavailable-API failures accurately. Identify permission/denial outcomes only from evidence that supports the claim; unknown exceptions and malformed responses must have neutral, actionable, privacy-bounded errors. Never expose clipboard text or raw exception content in error output.

Do not change permission policy, focus behavior, transport libraries, public requests/schemas, remote-object lifetime, or adjacent keyboard behavior. In particular, a general response-envelope framework and dual historical result shapes are unnecessary for this local fix. Existing timeout/disconnection/stale-document behavior should remain stable.

Verification: add tests through read/write operations with a scripted transport using the production result shape, including success, known failures, denial, unknown/malformed output, and privacy sentinels. Demonstrate a regression test failing against the old production logic, then green with the fix. Run scoped clipboard tests, the CDP crate test suite, formatting, and scoped Clippy. The parent independently reviews and reruns focused checks before acceptance. No Chrome launch or paid model-effectiveness run is required or claimed by this deterministic pilot.

Model trial observations belong in this item and the owning epic: record exact model/thinking, active runtime if available, first-pass failures, parent corrections, scope discipline, and verification outcomes. A single narrow task cannot establish general model quality.
