---
id: gate-tests-real-chrome-browser-context-seam
kind: story
stage: done
tags: [testing]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: tests
created: 2026-07-18
updated: 2026-07-18
---

# Qualify frame actions and page assets in real Chrome

## Priority
High

## Value evidence

Item: `epic-agent-browser-ergonomics-browser-contexts`

Owner-frame geometry, frame-scoped query/action, stale navigation fencing, and privacy-bounded Resource Timing form the riskiest browser-protocol seam. Existing coverage uses constructed frame trees and parsed asset rows but does not drive `list_frames` → frame `query_page` → referenced action or `list_page_assets` against Chrome.

## Gap type
e2e-seam

## Suggested test

Add one opt-in real-Chrome fixture that scrolls the root, queries/clicks a same-origin child-frame target, proves the reference stale after child navigation, and checks bounded sanitized asset metadata without raw query/fragment/path/content leakage.

## Test location
`crates/krometrail-cdp/tests/verified_interactions.rs`

## Acceptance

- One `KROMETRAIL_REAL_CHROME_TESTS=1` qualification uses the existing serialized real-browser harness and a loopback same-origin fixture.
- The test scrolls the root page, inventories a same-origin child frame, queries an exact semantic reference in that frame, clicks it, and proves the old reference is stale after child navigation.
- The same test loads more than 256 current-page resources and proves `list_page_assets` returns exactly the bound, reports omissions, and serializes only sanitized metadata without query, fragment, local path, or content leakage.

## Test notes

Keep default tests deterministic and Chrome-free. The opt-in lane must own a temporary profile, use the real-browser lock, and stop the session cleanly.

## Implementation notes

- Execution capability: focused inline real-browser qualification using the existing lock, temporary profile, production connector, and loopback fixture server.
- Review weight: bounded standalone-story review, per gate-bundle caller.
- Files changed: browser-context fixture pages, qualification static server, context adapter Resource Timing parameters, and `verified_interactions` real-Chrome test.
- Tests added: one opt-in chain explicitly navigates the bound target, polls `list_frames` as the sole frame-readiness authority, verifies a root-scrolled inherited-origin child frame, queries and clicks its exact semantic reference, observes stale rejection after child navigation, and waits for a 256-entry sanitized asset projection with omissions.
- Production correction: Chrome refuses `performance.getEntriesByType('resource')` under heuristic `throwOnSideEffect:true`. The expression is fixed and adapter-owned, so the operation now disables that heuristic while retaining the closed expression, browser-side sort/filter/cap, and privacy sanitizer.
- Qualification discovery: launch target metadata may advertise the requested URL while the attached renderer is still `about:blank`; the test uses the public navigation operation before treating frame inventory as authoritative.
- Simplification: no raw CDP fixture probe or readiness-expression fallback remains; sanitized `list_frames` and `list_page_assets` are the observation authorities.
- Discrepancies from design: the same-origin fixture uses `srcdoc` to exercise inherited origin deterministically, then navigates to a loopback child document for stale fencing.
- Adjacent issues parked: none.

## Verification

- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test verified_interactions opt_in_real_chrome_qualifies_frame_actions_staleness_and_bounded_assets --locked -- --nocapture` — passed against local Chrome.
- CDP context unit tests, default skipped registration, all-target check, and warning-denied clippy pass.

## Bounded inline review — 2026-07-18

- Verdict: approved. The opt-in lane crosses the production connector and public operations, asserts exact frame/reference authority and stale behavior, and inspects only sanitized bounded asset output. The Resource Timing change relaxes only Chrome's heuristic for one immutable adapter expression and does not expose caller evaluation.
