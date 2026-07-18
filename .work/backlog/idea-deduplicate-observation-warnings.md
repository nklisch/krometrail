---
id: idea-deduplicate-observation-warnings
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux, diagnostics]
---

# Deduplicate post-action observation warnings

During manual testing with Krometrail v1.0.3, clicking a public JavaScript-alert fixture correctly returned `degraded` because the open dialog blocked page, screenshot, and snapshot observation. The response's top-level `warnings` array repeated the exact same `page_observation_failed` warning three times with identical code, message, recovery, retry, and context, but no field identifying which observation component produced each copy. Dialog handling then succeeded normally. Deduplicate equivalent top-level warnings or preserve component ownership so the repetition carries information.
