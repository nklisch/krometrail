---
id: gate-cruft-clarify-stream-runtime-send-sync-assertion
created: 2026-07-15
updated: 2026-07-15
tags: [cleanup, browser, testing]
gate_origin: cruft
release_binding: null
---

Decide whether the intentionally uncalled `_assert_send_sync` compile-time guard in `capture/pipeline.rs` should be retained with a clearer explanation, converted to an invoked/static assertion pattern, or removed after proving existing spawned/Arc boundaries enforce both `Send` and `Sync`. The scanner classified it as dead code, but its typechecked body may still protect a useful compile-time guarantee; do not remove it as release cleanup without that adjudication.
