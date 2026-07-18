---
id: idea-fix-nested-frame-query
created: 2026-07-18
updated: 2026-07-18
tags: []
---

On `https://www.w3schools.com/html/tryit.asp?filename=tryhtml_iframe`, Krometrail 1.1.0 `list_frames` returned the result iframe and its nested `demo_iframe.htm` as `same_origin_same_process` with valid frame references. `query_page` scoped to the result frame returned `no_match` for the visible `HTML Iframes` heading, and a query scoped to the nested frame returned `no_match` for `This page is displayed in an iframe`. A main-document query for `Run ❯` remained uniquely successful. The Chrome-extension DOM snapshot included both nested headings; the in-app snapshot included the first-level heading but stopped at the innermost iframe. Correlations for the two qualified frame queries: `a0df5440-1671-4d01-9dfb-3e27bcd47037` and `2e8c57df-2bc7-4d37-a0d2-083e522746f5`.
