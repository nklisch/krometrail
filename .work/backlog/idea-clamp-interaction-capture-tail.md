---
id: idea-clamp-interaction-capture-tail
created: 2026-07-18
updated: 2026-07-18
tags: [browser]
---

`temporal_debug_bundle` anchored to a successful scroll interaction failed even with `after_ms: 0` and `retention: AllowPartial` because the interaction completed about 26 ms after the last retained damage-driven frame. A latest-interaction request on a static page failed in the same way. The recovery text provides captured bounds, but the ergonomic anchor cannot normally predict this acknowledgement-to-frame tail; consider resolving or explicitly clamping that natural anchor to available evidence while preserving the exact resolved range and limitation.
