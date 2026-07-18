---
id: idea-navigation-observes-before-viewport-restore
created: 2026-07-17
updated: 2026-07-17
tags: [bug, browser, visual, agent-ux]
---

Krometrail v1.0.5 manual testing with a 360x640, DPR 3, mobile/touch override exposed a
post-navigation evidence race on Wikipedia. `navigate_page` succeeded without warnings, but its
returned page observation and screenshot were captured before the asynchronous override replay:
the response reported a 1120x1991 visual viewport with page scale 0.3214. A later `inspect_page`
reported the restored 360x640 visual viewport with page scale 1, and `browser_status` showed capture
still healthy. Navigation should not advertise successful post-action evidence that contradicts the
declared target viewport; restoration and independent verification need to precede that observation,
or the response must explicitly degrade the premature evidence.
