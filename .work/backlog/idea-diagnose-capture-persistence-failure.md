---
id: idea-diagnose-capture-persistence-failure
created: 2026-07-18
updated: 2026-07-18
tags: [browser, diagnostics]
---

Capture can enter a terminal `frame_persistence` failure during ordinary public-site testing while retention capacity and local disk space remain available. The post-1.1.2 comparison pass first failed after frame-heavy navigation with 79 frames received and acknowledged, 56 accepted, and 55 persisted. A completely fresh managed-browser session reproduced the failure immediately: two frames received, zero persisted, state `failed`, failure stage `frame_persistence`, and retention still `available` with roughly 0.98 GB used of the configured 10 GB budget. The bounded diagnostic log entry exposed only `capture_failed` and `frame_persistence`, without a safe underlying persistence category or actionable cause, so an agent cannot distinguish a store lock, corrupt segment, filesystem error, or another recoverable condition. Stopping the first failed session returned `managed_browser_closed_degraded` without a warning or diagnostic reference, further obscuring recovery feedback.
