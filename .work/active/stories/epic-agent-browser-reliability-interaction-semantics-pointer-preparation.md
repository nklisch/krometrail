---
id: epic-agent-browser-reliability-interaction-semantics-pointer-preparation
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability-interaction-semantics
depends_on: [epic-agent-browser-reliability-interaction-semantics-reference-lifetime]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Prepare off-screen elements before pointer dispatch

## Checkpoint

Centralize element pointer preparation as resolve, scroll into view, re-resolve, and validate fresh
viewport geometry for click, hover, and drag. This checkpoint owns the off-screen interaction
portion of GitHub issue #11.

## Acceptance evidence

- [ ] Reference and selector pointer actions scroll off-screen actionable elements and dispatch at
      their fresh in-viewport coordinates.
- [ ] Hover and both drag endpoints share the same preparation rule.
- [ ] Scroll-triggered replacement or obstruction fails explicitly rather than dispatching stale
      geometry.
- [ ] Declared coordinate actions retain exact no-scroll semantics and hit-test recovery.

## Ordering and blocker

Depends on the document-scoped reference checkpoint. The dependency is a design/identity
constraint, not a request for separate implementation ownership.

## Implementation evidence

- Element pointer resolution now performs resolve, `DOM.scrollIntoViewIfNeeded`, re-resolve, and
  post-scroll viewport validation in the common interaction path used by click, hover, drag source,
  and drag destination.
- Coordinate locators retain their direct hit-test/no-scroll path.
- The scripted interaction qualification now supplies both pre- and post-scroll resolution and
  proves dispatch uses the fresh viewport geometry.
