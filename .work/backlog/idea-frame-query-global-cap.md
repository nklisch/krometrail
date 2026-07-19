---
id: idea-frame-query-global-cap
created: 2026-07-18
updated: 2026-07-18
tags: [browser]
---

A semantic `query_page` explicitly scoped to a supported same-origin iframe on the W3Schools iframe documentation page failed with `page_observation_failed: DOM snapshot exceeds the 5000-node semantic limit`. The requested frame was small, but the surrounding top-level page was large. Investigate whether frame-scoped acquisition can honor the requested document boundary before applying the global snapshot limit, or otherwise return guidance that explains why unrelated main-document size prevents the scoped query.
