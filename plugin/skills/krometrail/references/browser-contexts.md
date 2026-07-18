# Browser contexts, reusable profiles, and page assets

Use the smallest context surface that answers the task, then expand only when navigation crosses a
page, frame, or resource boundary.

- Call `list_managed_profiles` before launch when prior login or site state may matter. Results are
  reusable profile identities plus an `in_use` flag; they never expose profile paths or contents.
  Omitted launch profile selection keeps the ergonomic reusable `default` profile. Choose a named
  reusable profile only when the task needs its state, a temporary profile for isolation, and attach
  only when the user explicitly needs an already-running debug-enabled browser.
- Use `list_page_contexts` to take a page cursor before an action that may open a popup, then call
  `wait_for_page` with that cursor. Supply the opener's Krometrail target ID when unrelated pages
  must not satisfy the wait. These tools do not activate or focus a page.
- Call `list_frames` only when the relevant content is inside an iframe. Main-document and qualified
  same-origin/same-process entries may be inspected; cross-origin, out-of-process, stale, and
  indeterminate entries fail explicitly. Refresh the inventory after frame navigation. Never retry a
  failed frame action against main-document coordinates.
- For an unnamed control whose visible identity comes from its row, card, or other bounded ancestor,
  use a role query with `container_text`. This is a semantic ancestor relationship, not a spatial-nearness
  heuristic; narrow ambiguous outcomes before acting.
- Call `list_page_assets` when resource identity, kind, timing, or browser-disclosed sizes can explain
  a layout or loading issue. The inventory is capped at 256 entries, reports omissions, strips URL
  query/fragment data, and never contains headers, bodies, cookies, raw URLs, or local paths.

Prefer semantic queries and exact returned node references inside a supported document scope.
Frame, page, and node references are generation-scoped evidence, not durable locators.
