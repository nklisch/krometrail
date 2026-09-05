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

## Bounded pilot implementation and validation — 2026-09-05

Implemented by the scoped GLM-5.3-Flash xhigh trial (pi coding-agent sub-agent runtime; model and thinking level selected by Nathan for this trial) in worktree `.krometrail-flash-clipboard-trial` on `pilot/flash-clipboard-error-shape`. Scope held to `crates/krometrail-cdp/src/control/clipboard.rs` plus these notes; stage stays `implementing` for parent review.

### Implementation

- One production command-result representation. `clipboard_bridge_value` decodes the unwrapped `Runtime.callFunctionOn` outcome (`{"result": <remote object>, "exceptionDetails": <optional>}`) and inspects `exceptionDetails` first, so a rejected bridge call can never be accepted as a successful value.
- Classification from description evidence only: `secure_context_required` and `clipboard_unavailable` → `unsupported` with context-specific recovery; `focus_required` and the real-Chrome focus race `NotAllowedError: Document is not focused.` → `interaction_failed` with focus recovery; `NotAllowedError` (the `navigator.clipboard` permission-rejection DOMException) is the only evidence-backed permission denial; any other exception stays neutral ("unidentified in-page error") with generic retry advice.
- Malformed responses — an exception-free envelope with no `/result/value`, or a value of the wrong type for the operation — return a neutral uninterpretable-response error instead of the previous confident permission claim.
- Removed the legacy dual-shape fallbacks in `clipboard_execution_object` (the `frameTree` `unwrap_or`, `/result/executionContextId`, and `/result/result/objectId` alternates) per the one-envelope contract. Dispatch-death classification (timeout/protocol/disconnect) and the in-page bridge scripts are untouched.
- Privacy preserved and asserted: no raw description, class name, or clipboard text reaches any error message or recovery text.

### Verification (exact commands, worktree cwd)

- Red, old production logic: `cargo test -p krometrail-cdp --lib control::clipboard` — 8 passed, 6 failed. The six failures are the new transport-shaped regressions: secure-context misclassified as permission denial (`InteractionFailed` instead of `Unsupported`), write focus failure not focus-classified, document-not-focused race claimed as denial, unknown exception claimed denial, exception accepted as a successful read (`unwrap_err()` on `Ok(ClipboardRead)`), and malformed response claimed denial. Pre-existing tests and the behavior-preservation additions (success shape, byte limit, `NotAllowedError` denial) passed on the old logic.
- Green, fixed logic: same command — 14 passed, 0 failed.
- `cargo fmt -p krometrail-cdp -- --check` — clean after one correction iteration (first check failed on new-test line wrapping; fixed with `cargo fmt -p krometrail-cdp`).
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — clean.
- `cargo test -p krometrail-cdp --all-targets --locked` — all suites pass: lib 267 passed / 5 ignored (deliberate `#[ignore]` snapshot micro-benchmarks in `control/snapshot.rs`), every integration binary 0 failures.
- Shared `/storage/cargo-target` used throughout; no build directories created, nothing deleted. No Chrome launch, no user clipboard mutation, no paid model runs; the opt-in real-Chrome clipboard qualification was not executed.

### Model trial observations (single trial, no general-model claims)

- First-pass corrections, all caught by me before or during verification: a miscounted sentinel byte length in a success assertion (caught by self-review before the red run; first build cancelled); a write-transport test helper that double-consumed the scripted bridge response (caught pre-compile; helper removed); an overlapping edit batch that clobbered the `clipboard_failure` signature while removing `clipboard_response_error` (repaired in one follow-up edit); one edit dispatched to a typo'd file path and retried; the `cargo fmt --check` miss above.
- Scope discipline: red tests were written to compile against the old logic so the failure evidence is honest; no edits outside `clipboard.rs` and this item; no API, schema, policy, transport, or epic-topology changes.

### Remaining gaps for parent review

- `NotAllowedError` denial semantics rest on documented Chrome `navigator.clipboard` behavior, not a live browser run in this pilot; the opt-in real-Chrome clipboard qualification remains the live-coverage lane.
- Workspace-wide gates (`cargo test --workspace --all-targets`, wire-enum schema check) were outside the pilot scope and were not run.
- Acceptance-criteria checkboxes above are left for the parent to tick at review; the evidence for each is in this section.
