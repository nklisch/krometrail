---
id: idea-compact-batch-response-payload
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux, performance]
---

A two-step v1.0.5 batch (`scroll` then a 100ms elapsed wait) produced an agent response of roughly
13,000 tokens despite `include_step_screenshots: false`. The scroll step embedded a large live
snapshot and the batch then embedded another large final observation, including hundreds of
accessibility nodes plus omission metadata. The response is technically bounded but duplicates far
more current-state evidence than an agent needs to confirm this small batch, making ordinary batch
use expensive and difficult to inspect.
