---
id: default-touch-capture-geometry
kind: story
stage: backlog
tags: [browser, testing]
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-30
updated: 2026-09-30
---

# Default touch capability prevents capture geometry acquisition

During delivery of `attached-local-mock-page-not-listed`, an explicitly owned
NCU 0.4.0 desktop running Chrome 151.0.7922.137 reported default
`navigator.maxTouchPoints = 10`, without a Krometrail viewport override.
Direct CDP evaluation returned valid layout dimensions, scale, and viewport-meta
presence, with and without `throwOnSideEffect`. The `decode_effective_viewport`
branch for `declared: None` rejects any observed touch capability as “browser
did not clear touch emulation”. Capture geometry therefore fails before
screencast startup. The discovery story isolates that failure from page control;
it does not change viewport acknowledgement/clear semantics.

Evidence: `~/.cache/dng-workstations/attached-page-evidence/geometry.log`,
`geometry.mjs`, and the discovery story's stage-classified baseline notes.
Investigate how observation of browser defaults should differ from validation of
explicit override clearing while preserving the default-touch capability.
No priority or implementation design has been accepted.

The same no-override touch check also runs after `set_viewport` clear and during
viewport rollback (`session/operations.rs`, clear observation and
`rollback_viewport_or_fail_target`). On a default-touch host, a rollback
observation may reject the restored browser defaults and cause
`rollback_viewport_or_fail_target` to terminally fail the page. This is a
code-reading hypothesis; that rollback journey has not been run.
