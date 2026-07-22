---
id: epic-state-aware-interaction-results-side-channel-outcomes
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results
depends_on: [epic-state-aware-interaction-results-postcondition-core]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Side-channel outcomes

## Brief

Extends the postcondition block with side-channel outcome facts so an
interaction that should open a page, start a download, or produce a clipboard
result can be verified from its own result (issue #14 findings #8 and #9).
Three surfaces, each with a mapped gap:

- **New page/popup facts.** The target supervisor already assigns a monotonic
  `PageSequence` on adoption with opener relationships; the page cursor is
  deliberately never absent. A post-action page-cursor delta (new page adopted,
  opener matches the acting target) becomes an observed fact. Blocked
  `window.open` currently produces no signal at all — investigate whether a
  bounded negative/attempt signal is observable (e.g. a page-emitted open
  attempt event) and otherwise report the honest "no new page observed" fact.
- **Download facts.** The download authority is lazily activated and its
  inventory cursor is `Option` — absent until something is recorded — which is
  exactly the unusable state finding #9 hit (`list_downloads` empty with no
  cursor after an activation). Align the download cursor with the page-cursor
  "never absent" contract, resolve the lazy-activation interplay for
  interaction-time facts, and record a bounded outcome fact when download
  activity follows an interaction. A suppressed/never-begun download leaves no
  record today; design must decide what is honestly observable.
- **Clipboard facts.** Root-cause the finding #8 failure mode: a dispatch death
  classified as transport (`command_failed`) with no permission prompt and no
  way to distinguish product failure from browser limitation. Improve failure
  classification at the clipboard boundary and record explicit clipboard
  operations' outcomes as bounded facts.

Root-cause obligations from the epic body land here: both #8 and #9 may hide
concrete defects; reproduce with deterministic doubles and boundary fault
injection (layered-cdp-qualification) — the reporting surface must not paper
over a real bug.

## Advisory constraints (binding, from the epic's cross-model adjudication)

- **Supervisor serialization**: `Execute` is processed serially — queued
  `Input(TargetCreated)` events are handled only after the operation returns,
  so `page_contexts()` reflects pre-action state during interaction
  execution. Popup facts require a post-dispatch target-event reconciliation
  step (or assembly after target reduction); do not read the page cursor
  inside `execute_interaction_request_inner` and call it a post-state.
- **Clipboard scope narrowed**: enrich the explicit clipboard operations'
  records and fix failure classification (#8's `command_failed` ambiguity);
  no automatic clipboard probing after arbitrary clicks.
- **Download activation ordering**: the lazily-activated download authority
  must be subscribed/enabled before interaction dispatch or early downloads
  stay unobservable; cursor seeded like the page cursor (`next - 1`, never
  absent); `WaitForDownloadRequest.after` becomes required.
- **CDP signal qualification**: `Page.windowOpen` = bounded open-attempt fact
  (no blocked field); `Page.frameRequestedNavigation` with disposition
  `download` = download-attempt candidate; `Browser.downloadWillBegin`
  onward = lifecycle facts. There is no general suppressed-popup/download
  signal: report attempt/outcome/no-outcome-observed, never "blocked".
- **Bounded collections**: any per-interaction side-channel lists carry
  canonical caps and exact omission counts before serialization into the
  record (the record's byte discipline is enforced at construction).
- **Navigation-fact upgrade (advisory F5, accepted follow-up)**: one pre/post
  URL pair is a URL delta, not a navigation delta — it misses same-URL
  reloads and navigations that commit and return before observation. Back the
  navigation fact with an always-on, non-waiting main-frame
  `Page.frameNavigated` + `Page.navigatedWithinDocument` signal (cursor or
  drained receiver), keeping `url_changed` as a separate fact; record signal
  availability alongside.

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: consumer of `postcondition-core`'s block; producer of the
  side-channel facts `expectation-notes` reasons over.

## Simplification opportunity

- Reconcile the two cursor contracts: the page cursor is never absent by
  design, the download cursor is `Option`. One "cursor is never absent"
  contract deletes the absent-cursor special case and its recovery prose.
- Prefer facts derived from existing authorities (page inventory, download
  tracker, clipboard boundary) over any new parallel event stream.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (side-channel postconditions),
  Browser Lifecycle (pages), Local Data (downloads/clipboard boundaries)
- `docs/ARCHITECTURE.md` — Target Lifecycle, MCP Boundary
- GitHub issue #14, findings #8 (`7809bc9c-230d-4674-a7ea-befd309d4b21`) and
  #9 (`a6e7a7bd-340c-4fc6-a922-feabcd61a64a`)
