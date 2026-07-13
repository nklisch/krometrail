---
id: idea-clean-real-chrome-test-root-drop-order
created: 2026-07-13
updated: 2026-07-13
tags: [browser, testing, cleanup]
---

The opt-in capture fidelity tests can intermittently leave an empty `krometrail-real-reconnect-*` or multi-target test-root shell because `TestRootGuard` drops before the local `launched` value releases its `ProfileLease`. Browser processes and profile data are still removed, and the next test process self-cleans the empty known-prefix shell.

A future cleanup pass should explicitly `drop(launched)` before `drop(root)` in the affected real-Chrome tests and assert the root shell disappears. This is test-harness hygiene, not a production ownership or data-retention defect.
