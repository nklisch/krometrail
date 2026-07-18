---
id: idea-fix-screencast-geometry-provenance
created: 2026-07-17
updated: 2026-07-17
tags: [bug, temporal-evidence, viewport]
---

# Fix screencast geometry provenance after viewport override

Manual testing with Krometrail v1.0.3 set and verified a page at 600×500 CSS pixels, DPR 1, then reloaded and recorded a five-second dynamic-loading interaction without another viewport change. The 52 gap-free retained frames were split into three visual epochs: most frames had a 1200×1000 encoded image with 600×500 viewport metadata and DPR 1, while one transient frame had a 600×500 image with 300×250 viewport metadata and DPR 1. Live screenshots remained 600×500. The apparent 300×250 geometry transition did not correspond to a requested or observed layout change, so screencast dimensions or scale metadata appear to be treated as authoritative viewport geometry and create false visual epochs.
