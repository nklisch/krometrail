---
id: epic-state-aware-interaction-results
kind: epic
stage: drafting
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

## References

- GitHub issue #14 (E2E ergonomics: semantic interaction and retained-evidence
  verification gaps), findings 1, 5, 8, 9.
