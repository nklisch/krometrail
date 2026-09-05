---
id: epic-a-grade-reliability-clipboard-error-shape
kind: feature
stage: done
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

- [x] Use actual transport-shaped fixtures for focus_required, secure_context_required, clipboard_unavailable, permission denial, malformed results, and success.
- [x] Each known failure reports its accurate category and actionable recovery; unknown exceptions do not become a confident permission diagnosis.
- [x] Normalize the supported transport envelope consistently without proliferating legacy result variants.
- [x] Keep raw clipboard content and sensitive exception material out of diagnostic summaries.

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

Implemented by the scoped GLM-5.3-Flash xhigh trial (pi coding-agent sub-agent runtime; model and thinking level selected by Nathan for this trial) in worktree `.krometrail-flash-clipboard-trial` on `pilot/flash-clipboard-error-shape`. Scope held to `crates/krometrail-cdp/src/control/clipboard.rs` plus these notes; the implementer left the item for parent review; final acceptance is recorded below.

### Implementation

- One production command-result representation. `clipboard_bridge_value` decodes the unwrapped `Runtime.callFunctionOn` outcome (`{"result": <remote object>, "exceptionDetails": <optional>}`) and inspects `exceptionDetails` first, so a rejected bridge call can never be accepted as a successful value.
- Classification from the description's first `ClassName: message` line only (corrected after parent review; see below): `secure_context_required` and `clipboard_unavailable` → `unsupported` with context-specific recovery; `focus_required` and the verified focus-race message `NotAllowedError: Document is not focused.` → `interaction_failed` with focus recovery; Chrome's source-grounded "Read permission denied." / "Write permission denied." rejections → the only claimed permission denial; every other exception — including the other `NotAllowedError` shapes — stays neutral ("unidentified reason") with generic retry advice.
- Malformed responses — an exception-free envelope with no `/result/value`, or a value of the wrong type for the operation — return a neutral uninterpretable-response error instead of the previous confident permission claim.
- Removed the legacy dual-shape fallbacks in `clipboard_execution_object` (the `frameTree` `unwrap_or`, `/result/executionContextId`, and `/result/result/objectId` alternates) per the one-envelope contract. Dispatch-death classification (timeout/protocol/disconnect) and the in-page bridge scripts are untouched.
- Privacy preserved and asserted: no raw description, class name, or clipboard text reaches any error message or recovery text.

### Verification (exact commands, worktree cwd)

- Red, old production logic: `cargo test -p krometrail-cdp --lib control::clipboard` — 8 passed, 6 failed. The six failures are the new transport-shaped regressions: secure-context misclassified as permission denial (`InteractionFailed` instead of `Unsupported`), write focus failure not focus-classified, document-not-focused race claimed as denial, unknown exception claimed denial, exception accepted as a successful read (`unwrap_err()` on `Ok(ClipboardRead)`), and malformed response claimed denial. Pre-existing tests and the behavior-preservation additions (success shape, byte limit, `NotAllowedError` denial) passed on the old logic.
- Green, fixed logic: same command — 14 passed, 0 failed.
- `cargo fmt -p krometrail-cdp -- --check` — clean after one correction iteration (first check failed on new-test line wrapping; fixed with `cargo fmt -p krometrail-cdp`).
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — clean.
- `cargo test -p krometrail-cdp --all-targets --locked` — all suites pass: lib 267 passed / 5 ignored (deliberate `#[ignore]` snapshot micro-benchmarks in `control/snapshot.rs`), every integration binary 0 failures.
- Shared `/storage/cargo-target` used throughout; no build directories created, nothing deleted. No Chrome launch, no user clipboard mutation, no paid model-effectiveness runs; the opt-in real-Chrome clipboard qualification was not executed.

### Model trial observations (single trial, no general-model claims)

- First-pass corrections, all caught by me before or during verification: a miscounted sentinel byte length in a success assertion (caught by self-review before the red run; first build cancelled); a write-transport test helper that double-consumed the scripted bridge response (caught pre-compile; helper removed); an overlapping edit batch that clobbered the `clipboard_failure` signature while removing `clipboard_response_error` (repaired in one follow-up edit); one edit dispatched to a typo'd file path and retried; the `cargo fmt --check` miss above.
- Scope discipline: red tests were written to compile against the old logic so the failure evidence is honest; no edits outside `clipboard.rs` and this item; no API, schema, policy, transport, or epic-topology changes.

### Implementer-run coverage limits (parent results below supersede)

- Denial classification is now grounded in Chromium source (see the correction section below) rather than assumption, but it is still not a live browser run in this pilot; the opt-in real-Chrome clipboard qualification remains the live-coverage lane.
- The implementer did not run workspace-wide gates or the wire-enum schema check; the parent subsequently ran both successfully, as recorded below.
- The parent adjudicated the acceptance criteria after independent review and integrated verification; see final acceptance below.

## Parent review correction — 2026-09-05 (first-line classification)

The parent's review of `7effdbef` found a medium-severity correctness issue in the first-pass classifier: it equated `description.contains("NotAllowedError")` with a confirmed browser permission denial. The first-pass notes above claiming the DOMException name alone was evidence-backed denial were wrong and have been removed. [Chromium's clipboard promise source at `2c9f204f98324dcb771cb81b334b8fc96edb9da7`](https://github.com/chromium/chromium/blob/2c9f204f98324dcb771cb81b334b8fc96edb9da7/third_party/blink/renderer/modules/clipboard/clipboard_promise.cc#L761-L764) (fetched and read selectively; source inspection, not observed live-browser behavior) rejects with `NotAllowedError` for at least: "Permission Service could not connect." (lines 761-764), "Document detached." (815-818, 838-839), permissions-policy blocking (693-707), "Permission denied by system." (507/536, an OS-level refusal, not a browser permission decision), and the focus race "Document is not focused." (689). The only confirmed user-permission denials are "Read permission denied." (305/468) and "Write permission denied." (609/646).

### Correction implementation

- `clipboard_exception_error` now discriminates on the description's first `ClassName: message` line only. Stack-frame content — a `NotAllowedError` mention, a bridge sentinel, even a denial message — can no longer manufacture a known cause.
- The claimed-denial branch matches only the two source-grounded messages ("Read permission denied." / "Write permission denied."); exact-phrase matching also keeps "Permission denied by system." neutral, which is honest because its recovery differs from a browser permission grant.
- The unsupported `exceptionDetails.description` fallback was removed; `exception.description` is the only description location in the current CDP shape, and a details object without it stays neutral.
- The neutral message was reworded to "clipboard operation failed for an unidentified reason" — some neutral causes are browser-side, not in-page.
- Superseded test `denial_is_reported_only_from_not_allowed_error_evidence` was removed; the name had encoded the incorrect claim.

### Correction verification (exact commands, worktree cwd)

- Red against the first-pass logic: `cargo test -p krometrail-cdp --lib control::clipboard` — 17 passed, 2 failed. The failures are the requested regression demonstrations: `permission_service_failure_stays_neutral` (first-pass logic claimed "denied" for the service-connection failure) and `unlisted_rejections_stay_neutral_across_operations` (first failure message: "read/detach must not claim denial"). The other new cases — operation-level `clipboard_unavailable`, wrong-type/malformed success values on both paths, source-grounded Read/Write denials — passed against first-pass logic as well and are coverage additions, not regression claims.
- Green with the correction: same command — 18 passed, 0 failed (the superseded duplicate test was removed).
- `cargo fmt -p krometrail-cdp -- --check` — clean (formatting applied before the check this iteration).
- `cargo clippy -p krometrail-cdp --all-targets --locked -- -D warnings` — clean.
- `cargo test -p krometrail-cdp --all-targets --locked` — all suites pass: lib 271 passed / 5 ignored (the same intentional snapshot micro-benchmarks); every integration binary 0 failures. This supersedes the first pass's 267-test library count.

### Correction iteration observations

- The substantive first-pass defect was the over-broad denial claim itself — caught by parent review, not by the trial's own tests, because the trial's fixtures only exercised the NotAllowedError message the classifier expected. The correction's new fixtures now include the non-denial NotAllowedError shapes so that class of gap is closed locally.
- Mechanical correction iterations: one edit batch was rejected atomically on a wrong-neighbor `oldText` (superseded test sits before the byte-limit test, not the missing-document test) and reapplied with exact text; formatting was applied proactively this iteration to avoid a second fmt-check failure loop.
- Scope held: only `clipboard.rs` and this item changed; no taxonomy was added for every Chromium failure — unlisted causes stay neutral per the review direction.


## Parent acceptance and model assessment — 2026-09-05

**Disposition: accepted with review corrections; this feature is done.** The remaining reliability backlog is not activated by this pilot. The original trial commits `7effdbef` and `c93b886a` were integrated as `b0060e57` and `f2fd5934`, respectively; the parent then tightened the final discriminator.

### Independent review and verification

The first-pass permission-denial issue was corrected by Flash after source-backed parent feedback. The follow-up still used substring checks on the first description line while its report called them exact matching. The parent replaced those checks with a direct match on the complete supported `Error: ...` / `NotAllowedError: ...` first lines. Unknown text quoting a marker or permission message is now neutral, as are suffixed markers. Existing helper fixtures were corrected to carry the real `Error: ` description prefix.

- Parent regression before the final tightening: `cargo test -p krometrail-cdp --lib control::clipboard::tests::unlisted_rejections_stay_neutral_across_operations --locked` — failed as expected, 0 passed / 1 failed: `read/first-line-denial-quotation must not claim denial`.
- Final integrated `cargo fmt --all -- --check` — passed.
- `bash scripts/check-wire-enum-schemas.sh` — passed.
- `cargo test --workspace --all-targets --locked` — passed, including the extended first-line quotation/suffix regressions. Existing ignored benchmark and opt-in browser boundaries remain; this does not establish live Chrome execution.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- Host commands reported rustc/cargo 1.96.1. These checks do not establish the separately broken declared-minimum Rust 1.85 contract; that remains its own open item.
- No browser was launched, no actual clipboard was accessed, and no model-effectiveness experiment was run. No custom build directory was created; the existing shared Cargo cache remains intact.

### What this trial says about Flash

- Model: `zai/glm-5.3-flash`. Requested thinking: `xhigh`; the harness reported the effective level as `max`. Do not present this as proof of a distinct native xhigh mode.
- First run: 1047.5 seconds (17m 27.5s), 89 reported tool calls, 194806 reported total tokens. Follow-up run: 579.9 seconds (9m 39.9s). Combined active runtime: approximately 27m 7s, excluding the parent's review/integration work. The follow-up notification reported counters of 127 tool calls and 246735 total tokens; these appear cumulative and are not added to the first notification's counters. These are harness-reported usage figures, not a price or uncached-token calculation.
- Strengths: stayed within the assigned source/item files; implemented exception-first decoding; produced genuine red-to-green tests; ran the requested gates; acknowledged mistakes; used the review's Chromium evidence to correct a faulty assumption without expanding into permission policy or a new framework.
- Weaknesses: the first pass encoded an assumed browser cause into its fixtures and missed non-denial `NotAllowedError` cases. The correction report overstated substring matching as exact; the parent still had to tighten it. Several mechanical mistakes were self-corrected, and the run was not demonstrated to be faster or cheaper than Astra. Mandatory repository onboarding and gate execution are included in the timing, so this is not pure coding latency.
- Decision: **use Flash xhigh as the default implementer for scoped medium-complexity units, with Astra review**. Keep complex cross-cutting design/integration with Astra medium; Astra low remains suitable for medium-complexity implementation. One accepted narrow trial does not justify unreviewed correctness, concurrency, provenance, or permission-policy sign-off, nor a claim of general model superiority.
