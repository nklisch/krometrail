---
id: idea-recording-store-operational-edges
created: 2026-07-14
updated: 2026-07-13
tags: [storage]
---

Lower-risk operational follow-ups from the durable-browser-memory aggregate review:

- `RecordingStore::status()` intentionally serializes behind the shared mutation gate, so status latency can rise during eviction or session deletion. Revisit only if evaluation shows that latency is material.
- `RecordingStore::new` / `with_budget` rely on the composition root to call `recover()` before reopening a used index. Consider making that caller invariant harder to miss for future direct constructors and tests without coupling recovery into retention.
