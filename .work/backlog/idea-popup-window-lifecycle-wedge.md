---
id: idea-popup-window-lifecycle-wedge
created: 2026-07-19
updated: 2026-07-19
tags: [bug, browser]
---

Found in the 2026-07-19 motion workload (dev build of main at v1.2.3-19, foreground
managed session, local http target). `window.open('detail.html', 'detail',
'width=420,height=300')` from a button click produces a cascade of defects:

1. **Popup initial navigation never commits.** The OS window opens but the target
   stays at an empty URL / `about:blank` indefinitely (Chrome `/json/list` showed a
   `page` target with `url: ""`; `Page.getNavigationHistory` one empty entry;
   `document.readyState "complete"` on the blank document). `waitForDebuggerOnStart`
   is false in both attach configs, so it is not a debugger hold — the popup's own
   renderer-initiated navigation appears to be cancelled while krometrail's target
   discovery attaches/rejects the empty-URL target (diagnostics show
   `browser.target.attached` churn on the opener). A manual out-of-band
   `Page.navigate` on the frozen target loaded instantly, after which krometrail
   supervised it with the correct `opener_target_id` — proving the window was
   healthy and supervision works once a recordable URL exists.
2. **Opener click hard-fails instead of degrading.** Each popup-opening `click`
   returned the hard error "browser rejected or could not complete the page
   observation command" (no interaction record, no dispatch confirmation), even
   though the input actually dispatched (the window opened). `wait_for_page` then
   times out because the frozen popup never becomes supervisable.
3. **Post-close observation wedge on the opener.** After closing the popup page,
   the opener target silently detached/re-attached (log 16:03:02, attachment
   generation unchanged at 1) and every subsequent observation failed with
   `invalid_input: "CSS size must be finite and positive"` (`recovery: null`,
   `retry: never`) plus `browser.compositor.signal_unavailable` at
   `compositor_readiness` — through `observe_live`, same-origin `reload_page`
   (twice, including bypass_cache), and batch final observations, while the page
   itself stayed live and interactive (evaluate worked; snapshots returned zero
   targets). A **cross-origin navigation** (process swap) fully recovered
   observation. The `retry: never` / null recovery labeling is wrong for a state
   that is recoverable by navigation.

Also observed: `evaluate_page` on the adopted popup ran in the stale `about:blank`
execution context (null `document.documentElement`, `location` showing the new URL)
— context selection should track the current document.

Needs root-cause in target discovery/supervision for OS-window popups: don't cancel
or starve the popup's initial navigation while it is unsupervisable, degrade (not
hard-fail) the opener's post-action observation, re-baseline observation state on
target re-attach, and label the CSS-size failure with a truthful recovery.
