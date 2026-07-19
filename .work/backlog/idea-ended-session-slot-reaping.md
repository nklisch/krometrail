---
id: idea-ended-session-slot-reaping
created: 2026-07-19
updated: 2026-07-19
tags: [browser, agent-ux]
---

Found in the 2026-07-19 post-fix live shakedown: closing the last supervised page
exits Chrome (expected browser behavior) and leaves the session at
`browser_status.state: "ended"`, but the dead session still occupies the singleton
slot. `start_browser` then fails with "a browser session is already active", and
the recovery path is a `stop_browser` call that itself returns the error "browser
supervision task ended" while actually reaping the slot — an error-shaped response
for an operation that succeeds. Related friction in the same lifecycle corner:

- The last-page `close_page` response warns "no browser page remains selected
  after closure" with `recovery: null`; it does not say the browser session ended
  or point at `start_browser`.
- Post-ended operations fail with "browser supervision task ended" and no recovery
  action.

Fix direction: reap an ended session automatically on `start_browser` (or make
`stop_browser` on an ended session a success that reports the cleanup), and give
the last-page-close and ended-session errors recovery guidance naming
`start_browser`.
