---
id: epic-agent-browser-reliability-interaction-semantics-input-contracts
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-interaction-semantics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Correct request defaults, key dispatch, and fill replacement

## Checkpoint

Make page-scoped requests default to the selected page, canonicalize validated key chords, dispatch
normal Chrome key events, and replace editable contents without platform shortcuts or secret
exposure. This checkpoint owns GitHub issues #7 and #8 plus the target-default portion of #11.

## Acceptance evidence

- [ ] Generated schemas and serde accept omitted selected-page and conventional interaction
      options while preserving all explicit stable 1.x requests.
- [ ] Aliases canonicalize, malformed multi-key/duplicate-modifier chords fail before dispatch,
      and Control/Meta shortcuts emit no text-bearing char event.
- [ ] Enter activates/submits focused controls in real Chrome and Space retains native behavior.
- [ ] Replace-mode password fill clears first and verifies only zero length; no secret appears in
      logs, errors, command expressions, snapshots, or test output.

## Ordering and blocker

Independent first checkpoint. It does not depend on the reference-registry or pointer-preparation
changes and must preserve the interaction result's distinction between dispatched input and
observed page effect.
