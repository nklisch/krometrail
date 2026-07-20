---
id: idea-fix-qualified-frame-query
created: 2026-07-20
updated: 2026-07-20
tags: [browser, testing]
---

On `https://the-internet.herokuapp.com/iframe`, wait for TinyMCE to create its same-origin `about:blank` editor frame. `list_frames` then advertises the child as `same_origin_same_process`, but `query_page` scoped with that exact frame reference returns `no_match` for both `Your content goes here.` and the accessible rich-text-area identity. The Codex in-app browser snapshot exposes the iframe body and its paragraph, so the content is present and semantically observable. Diagnose why a qualified frame scope is empty and make the advertised frame/query contract agree.
