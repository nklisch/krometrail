---
id: mockup-browser-session-ended
kind: story
stage: backlog
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-06
updated: 2026-09-06
---

# Browser capture session ended during mockup review

- Kind: friction
- Status: open
- Observed: 2026-09-06
- Surface: external Krometrail browser tooling

## Goal

Save review screenshots of the desktop-monitor HTML mockup after the user viewed it.

## Observation

`navigate_page` on the temporary managed browser returned `cancelled`: “browser supervision task ended”. Recovery advised starting a new browser session. Correlation: `36afe872-b39a-436d-a332-78187b3f48d0`. The session had successfully displayed the mockup with Chrome 151.0.7922.137.

The reason the browser ended is unknown; the user may have closed the preview window. This is not evidence of an NCU defect or a confirmed Krometrail defect.

## Workaround

Saved static screenshots with headless Chrome using disposable profiles, removed automatically after each capture. No real desktops were controlled. Retain only if this workflow interruption recurs; no product change is proposed.


## Relocation context

Relocated from NCU under the user's explicit request. Retain as a bounded browser-lifecycle observation, not a confirmed production bug. No reproduction or implementation is commissioned by moving this report.
