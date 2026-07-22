---
id: epic-state-aware-interaction-results
kind: epic
stage: done
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# State-aware interaction results

## Brief

GitHub issue #14 — a consolidated report from a full E2E run of Krometrail 1.4.0
against an authenticated local React application — identifies one workflow-level
gap as its highest-value ask: an interaction reports successful dispatch, but the
caller cannot reliably establish whether the intended semantic or browser effect
occurred without CSS fallbacks, DOM inspection, or expensive temporal analysis.
A click that silently no-ops looks identical to a click that worked.

This epic makes immediate interaction results trustworthy: retain the current
visual evidence, and pair successful dispatch with bounded observed postcondition
data — plus a visual-completeness signal — so navigation, selection, expansion,
popup, download, and clipboard behavior can be established from the action result
itself. Compact temporal evidence remains the escalation path for the residual
ambiguous cases, not the entry fee for every verification.

## Strategic decisions

- **Report observed deltas plus a bounded conservative warning**: interaction
  results report observed pre/post deltas (URL/navigation, checked/expanded/
  selected state, target/backing-node identity, side-channel outcomes) and may
  add at most one conservative expectation note when a common expectation
  observably did not hold (for example, a link activation with no navigation).
  The note stays descriptive and never claims the action failed — consistent
  with the project's interpret-conservatively stance while answering the
  issue's ask for a signal on likely-unintended outcomes.
- **Postconditions are on by default, bounded**: every successful interaction
  carries a compact postcondition block in the concise projection. The issue's
  core complaint is that silent no-ops are indistinguishable from success;
  opt-in reporting reproduces the "didn't know to ask" failure. Cost is bounded
  the same way existing concise projections are.
- **V1 includes side-channel outcomes**: navigation/URL, checked/expanded/
  selected state, and node identity ship alongside new-page/download/clipboard
  outcome facts in the same epic arc, covering findings #8 and #9's
  observability gaps. Epic-design sequences these into features.

## Issue #14 findings covered

- **#1 — pointer actions can report success without semantic activation.** A
  visible navigation link clicked successfully but stayed on the same route
  (Enter on the same reference navigated); styled radio clicks left checked
  state unchanged (Space worked); a unique role/name button click navigated to
  an unexpected route with no warning. Interaction correlation:
  `a7ee197f-01b8-470a-af45-481b978f6445`.
- **#5 — immediate interaction imagery can contain compositor artifacts.** One
  post-action image showed overlapping duplicate cards; a temporal bundle with
  60 gap-free frames showed a clean transition, disproving the apparent product
  defect. The arc includes a visual-completeness/compositor-stability marker on
  immediate imagery so callers know when retained evidence is required before
  reporting a visual defect. Interaction:
  `44b8a67f-e2bb-49eb-9526-5af6bfd745c7`.
- **#8 — clipboard verification failed without a usable fallback state.** After
  activating a copy control, `read_clipboard` failed with a clipboard script
  dispatch transport error (`command_failed`), no permission prompt visible, and
  no durable state distinguishing product failure from browser limitation.
  Correlation: `7809bc9c-230d-4674-a7ea-befd309d4b21`.
- **#9 — popup/download fallback could not be verified.** After a blocked
  new-tab attempt, activating a direct open/download fallback link reported
  successful dispatch, but no managed page appeared and `list_downloads`
  remained empty with no cursor; the run could not establish whether the
  fallback opened, downloaded, or was silently suppressed. Interaction:
  `a6e7a7bd-340c-4fc6-a922-feabcd61a64a`.

## Investigation obligations

Findings #8 and #9 each carry a possible concrete-defect half beyond the
reporting gap, observed on macOS against an application we cannot reproduce
locally:

- The clipboard dispatch dying with `command_failed` before the page could
  respond, with no permission prompt, may be a real defect in clipboard
  execution or its failure classification.
- A download-triggering activation leaving `list_downloads` empty **with no
  cursor** may indicate the download authority missed a popup-blocked or
  suppressed flow entirely.

Epic-design must root-cause both paths (deterministic doubles and boundary
fault injection per the layered-cdp-qualification pattern) rather than treating
them purely as new reporting surface.

## Simplification opportunity

The existing bounded `semantic_outcomes` list on automatic live observations
describes current post-action state and explicitly does not claim a pre/post
change. A postcondition block overlaps that role; design should consider
consolidating into one bounded post-action semantic surface rather than adding
a parallel list. The unchanged-generation snapshot marker is adjacent prior art
for cheap change/no-change signaling.

## Foundation-doc roll-forward

- `docs/SPEC.md` Current-State Observation: interaction results carry a bounded
  on-by-default postcondition block, at most one conservative expectation note,
  and a visual-completeness marker on immediate imagery.
- `docs/VISION.md` Core Experience: interaction results pair dispatch with
  observed postconditions so effect-verification does not require a separate
  investigation.

## Design decisions

Resolved with judgment under the active autopilot goal (evidence: two read-only
codebase maps, key seams verified in source):

- **Postconditions live on `InteractionRecord`**: the record is persisted as
  opaque `record_json`, so postcondition facts reach the retained timeline
  automatically with no store migration; the response projects the same record.
  Least irreversible placement, one authority.
- **Pre-state rides the existing actionability pre-flight**: the
  `resolve_backend_node` `callFunctionOn` already computes per-target state and
  discards it; widening its payload and `ResolvedNode` yields
  checked/expanded/selected/value pre-state at near-zero cost. The only
  net-new pre-dispatch read is bounded page identity (URL). No full pre-action
  snapshot — that would violate response/observation economy.
- **Postcondition facts follow the `SanitizedParameters` privacy discipline**:
  booleans, bounded enums, lengths, opaque ids; never raw values or page
  content.
- **Expectations extend the existing operation registry** (`ActionDefinition`
  table), keyed by action kind and target role — registry-declared-surfaces,
  no parallel table.
- **Download cursor aligns to the page-cursor "never absent" contract**: the
  page cursor is seeded so it always exists; the download cursor is `Option`
  and absent until an event is recorded — the exact unusable state finding #9
  hit. One contract, absent-cursor special case deleted.
- **Visual-completeness v1 surfaces observed compositor-signal state only**
  (the double-rAF wait outcome, currently tracing-only) via the existing
  `EncodedScreenshot` warning surface; no pixel analysis on the immediate path
  — temporal-vision remains the analysis authority.

## Decomposition

Split by capability layer with facts before interpretation: one foundation
feature establishes the postcondition block on the record, one extends it with
side-channel facts (and owns the #8/#9 root-cause obligations), one small
interpretive feature derives the conservative expectation note from those
facts, and one independent feature surfaces the already-observed compositor
signal on immediate imagery. Alternatives considered: folding notes into the
core (rejected — the false-signal risk deserves its own design pass over
proven facts) and a per-surface split of side-channel (rejected — three thin
features sharing one fact-block contract).

### Child features

- `epic-state-aware-interaction-results-postcondition-core` — bounded pre/post
  delta block on the interaction record (URL/navigation, node identity,
  control state) — depends on: `[]`
- `epic-state-aware-interaction-results-side-channel-outcomes` — new-page,
  download, and clipboard outcome facts; #8/#9 root causes; cursor-contract
  alignment — depends on: `[postcondition-core]`
- `epic-state-aware-interaction-results-expectation-notes` — at most one
  conservative registry-declared expectation note over observed facts —
  depends on: `[postcondition-core, side-channel-outcomes]`
- `epic-state-aware-interaction-results-visual-completeness` — compositor
  visual-completeness marker on immediate imagery — depends on: `[]`

### Simplification arcs

- `postcondition-core` — evaluate consolidating the MCP-layer
  `semantic_outcomes` list into one bounded post-action semantic surface.
- `side-channel-outcomes` — one never-absent cursor contract across pages and
  downloads; facts derived from existing authorities, no parallel event
  stream.
- `visual-completeness` — replace the tracing-only compositor-signal discard
  with the surfaced marker.

### Decomposition risks

- **Riskiest feature: `postcondition-core`** — per-action overhead and
  contract blast radius. Mitigated: pre-state from already-executing calls
  plus one bounded URL read; the block is byte-bounded like
  `SanitizedParameters`.
- **False-signal risk concentrates in `expectation-notes`** — deliberately
  last, smallest, and pure over proven facts; conservative registry with no
  sensitivity knobs in v1.
- **Side-channel blind spots may be genuinely unobservable** (blocked
  `window.open` and suppressed downloads leave no CDP record today). The
  honest fallback is "no outcome observed" facts, not claimed blockage; the
  macOS report cannot be reproduced locally, so deterministic doubles and
  fault injection carry the verification load.
- **Critical path is core → side-channel → notes** (three deep); accepted
  because notes is small and visual-completeness parallelizes with core.

## Advisory review adjudication (cross-model, gpt-5.6-sol)

One advisory pass over the decomposition and code seams; every proposal
adjudicated by the receiving orchestrator:

- **Accepted, fixed immediately**: a required `postcondition` field makes
  pre-existing persisted `record_json` rows undecodable with no version
  mismatch to trigger cache clearing — store schema bumped to v9 so
  incompatible retained cache clears per Current Contract Discipline.
- **Accepted, bound into `side-channel-outcomes`**: (a) the supervisor
  processes `Execute` serially, so `page_contexts()` reflects pre-action state
  during interaction execution — popup facts need post-dispatch target-event
  reconciliation or assembly after target reduction; (b) any side-channel
  collections must carry canonical caps + omission counts before
  serialization; (c) clipboard scope narrows to explicit clipboard operations
  (record enrichment + failure classification) — generic clicks cannot
  observe clipboard state without an automatic probe with unresolved
  permission/privacy semantics; (d) download-cursor fix must also activate
  the download authority before interaction dispatch (lazy activation
  otherwise misses early downloads), seed the cursor like pages, and qualify
  `Page.windowOpen` (open-attempt fact, no blocked field) and
  `Page.frameRequestedNavigation` (disposition `download`) — never claim
  "blocked", only attempt/outcome/no-outcome.
- **Accepted, bound into `expectation-notes`**: negative notes require a
  per-channel completeness gate — typed channel states
  (changed/unchanged/unavailable/not-applicable + observed-through) and a
  note only when every channel the expectation requires was successfully
  observed; role-based expectations suppressed when the target role is
  unavailable.
- **Accepted, bound into `visual-completeness`**: rendezvous semantics, not
  completeness semantics — a successful double-rAF proves two callbacks ran,
  not that pixels are artifact-free; the marker/naming must report
  compositor-rendezvous provenance (observed/unavailable) and never map
  success to "visually complete".
- **Accepted as follow-up under this epic**: one pre/post URL pair is a URL
  delta, not a navigation delta (misses same-URL reloads and
  committed-and-returned navigations); an always-on non-waiting main-frame
  `Page.frameNavigated`/`Page.navigatedWithinDocument` signal should back the
  navigation fact, with URL comparison retained as a separate fact.
- **Rejected (already satisfied)**: "pre-state must come from the final
  post-scroll resolution" — the landed code re-resolves after scroll and uses
  only that resolution's facts; value facts are length-only.

## References

- GitHub issue #14 (E2E ergonomics: semantic interaction and retained-evidence
  verification gaps), findings 1, 5, 8, 9.

## Aggregate review (fresh-context Opus, deeper epic pass)

No blockers, no significant findings. Verified: end-to-end capability for all
five issue-#14 scenarios from the concise result; no fact/note contradiction
(attempt demotion, structural one-note cap); final type surface matches SPEC;
one coherent v12 persisted format with allowlisted clearing; bounded
documented per-interaction overhead with no double-pay; workspace gate green
at closure.

Four accepted minor/nit findings fixed in-stride before closure: the stale
probe comment (now states Unobserved semantics), SPEC's clipboard-outcome
sentence (scoped to the persisted write-clipboard record), removal of the
dead `ObservationSource`/`observed_through` plumbing with a simplified
evaluator match, and VISION's selection wording (control-state change).
Epic closed 2026-07-22.
