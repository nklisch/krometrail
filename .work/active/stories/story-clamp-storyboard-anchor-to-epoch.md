---
id: idea-clamp-temporal-bundle-artifact-anchor
created: 2026-07-17
updated: 2026-07-17
tags: [visual, agent-ux]
---

An exploratory plugin run requested `temporal_debug_bundle` around a successful `navigate_page` interaction with a 250 ms before / 3000 ms after window. The resolved interval retained 24 ordered source frames with no gaps, but the natural interaction anchor (`179736803208`) preceded the first retained frame (`179993802291`). The bundle succeeded only partially: difference-map generation worked, while storyboard and before/during/after both became unavailable with `artifact_generation_failed: storyboard anchor must lie inside the source frame range`. Make the high-level bundle select or clamp to a viable source-frame artifact anchor when its semantic anchor falls just outside otherwise valid retained evidence, or return specific recovery guidance that lets an agent retry correctly.
