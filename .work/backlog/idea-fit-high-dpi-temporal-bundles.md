---
id: idea-fit-high-dpi-temporal-bundles
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux, temporal-evidence]
---

# Fit default temporal bundles on high-DPI pages

During manual testing with Krometrail v1.0.3, a 5.4-second dynamic-loading interval resolved successfully with 53 retained frames, no capture gaps, and a native 1200×705 CSS viewport at DPR 2. The bundle itself succeeded, but default artifact generation was unavailable with `resource_limit_exceeded`: “decoded sequence bytes exceed the configured limit.” The artifact error supplied neither recovery guidance nor a safe retry path, leaving the agent to guess whether to shorten the range, lower the viewport, or request progressive evidence.
