---
id: idea-pointer-false-target-hidden
created: 2026-07-17
updated: 2026-07-17
tags: [browser, agent-ux]
---

Pointer interaction intermittently reports `target_hidden` on a managed, selected React.dev page even
while `browser_status` classifies the same sole target as `visible` and a page evaluation reports
`document.visibilityState: visible`, `document.hidden: false`, and `document.hasFocus(): true`.

The advertised recovery did not immediately work: `select_page` succeeded and reported the already
selected target as selected, but retrying the same reference click and then the equivalent CSS-selector
click both returned `target_hidden`. `press_keys` with `Enter` on the reference succeeded, and a later
pointer click on the next React page also succeeded, so this was an intermittent pointer activation
failure rather than a hidden or detached document. The bounded diagnostic for a failed retry was
correlation `70c94dab-0942-4113-b5ee-99cdafd84b9f`, route `click`, failure stage `operation`, stable
error `target_hidden`.
