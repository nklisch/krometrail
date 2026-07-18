---
id: idea-bound-auto-markers-to-source-frame-range
created: 2026-07-17
updated: 2026-07-17
tags: [visual, agent-ux]
---

After an interaction-anchored temporal bundle produced valid retained frames but unavailable storyboards because its semantic anchor preceded the first frame, an exploratory plugin run retried `temporal_debug_bundle` using the returned first and last source-frame IDs with `RequireComplete` and `Reject`. The request failed with `invalid_input: artifact marker is outside the resolved range`, even though the caller supplied no markers. Automatically selected interaction/navigation markers outside an exact source-frame range should be omitted, bounded, or represented as nearby context without invalidating the request. This currently blocks the most obvious agent recovery path. Diagnostic correlation: `43dab22b-0a81-467c-89c3-f07c1f80a354`.
