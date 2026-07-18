---
id: idea-fix-asset-kind-classification
created: 2026-07-18
updated: 2026-07-18
tags: []
---

During the post-1.1.0 cross-surface pass on Krometrail's public documentation, `list_page_assets` misclassified `.woff2` font files and multiple `.js` chunks as `stylesheet`. The in-app and Chrome-extension page-asset inventories independently reported the page as 7 scripts, 2 stylesheets, 2 fonts, 1 image, and 1 other asset, while Krometrail returned 10 stylesheet entries including obvious font and JavaScript extensions. Preserve the privacy-sanitized URL representation and bounded inventory, but revisit how Resource Timing initiator data, extension hints, and asset kind are reconciled so `kind` is diagnostically reliable.
