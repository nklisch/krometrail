---
id: idea-guide-temporal-window-bounds
created: 2026-07-17
updated: 2026-07-17
tags: [agent-ux, temporal-evidence]
---

# Guide temporal windows to captured bounds

During manual testing with Krometrail v1.0.3, a temporal debug bundle anchored to a successful click requested 500 ms before and 6 seconds after the interaction. The requested end was about 1.06 seconds beyond the target's latest captured source frame, so the bundle failed with `not_found` and “requested interval extends beyond captured source-frame bounds” even with `retention: AllowPartial`. Diagnostics were present, but recovery was null and retry was `never`; manually reducing the after-window to 4.9 seconds succeeded. Make this common capture-edge failure easier for an agent to recover from.
