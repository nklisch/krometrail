---
id: idea-harden-session-edge-semantics
created: 2026-07-13
updated: 2026-07-12
tags: [browser]
---

Review follow-ups below the current feature's material blocker bar:

- Represent slow-subscriber refresh guidance in a structured recovery field rather than only the `SubscriberLag` error message.
- Consider a graceful `Browser.close` attempt before managed process termination on explicit cancellation; current cleanup remains ownership-safe.
- Consider making `BrowserSessionPort::stop()` idempotently return the previously observed terminal outcome when called after process death or reconnect exhaustion rather than returning `cancelled` after the command task has ended.
- Consider stale reusable-profile lease recovery metadata (PID/lease time) for crash leftovers; current exclusive locking and data preservation are correct but require manual stale-lock cleanup.

These are valid robustness/agent-UX improvements but are not required by the landed target-supervision acceptance contract and do not compromise current cleanup or session correctness.
