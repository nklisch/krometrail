---
id: idea-omit-disabled-batch-screenshots
created: 2026-07-18
updated: 2026-07-18
tags: [agent-ux, browser]
---

`batch` emits a full per-step `screenshot.unavailable` error object even when the caller explicitly sets `include_step_screenshots: false`. This reproduced in both a five-site navigation batch and a five-step viewport batch on Krometrail 1.1.2. Every successful step carried `code: unsupported`, target context, message, retry classification, and null recovery fields solely to say that screenshots were not requested. The repeated negative structures dominate otherwise compact successful batch results and make an intentional economy option look like five unsupported outcomes.
