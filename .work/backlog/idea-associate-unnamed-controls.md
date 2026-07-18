---
id: idea-associate-unnamed-controls
created: 2026-07-18
updated: 2026-07-18
tags: []
---

During the post-1.1.0 cross-surface manual pass on `https://todomvc.com/examples/react/dist/`, two created todo rows exposed unnamed item checkboxes. `query_page` by checkbox role correctly returned the toggle-all checkbox plus both unnamed item checkboxes as ambiguous, but there was no semantic way to associate one checkbox with the adjacent rendered todo text. An exact text query for `compare browser surfaces` returned `no_match`; a contains query returned only the `RootWebArea`, which is too broad to use as a useful descendant scope. The in-app and Chrome surfaces could target the desired checkbox by scoping through the containing `li`. Revisit whether Krometrail should expose a bounded semantic ancestor/container relationship or rendered-text scope reference for controls whose accessible name is absent.
