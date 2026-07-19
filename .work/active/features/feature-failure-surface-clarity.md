---
id: feature-failure-surface-clarity
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Name the actual failure cause at the MCP boundary

## Brief

Four failure surfaces observed in a live shakedown return messages that hide the
information the caller needs to recover:

1. **`evaluate_page` conflates causes and erases the exception.** A thrown
   `Error('deliberate test error')` and a refused mutation (`document.title = 'x'`) both
   return the identical string "page evaluation raised an exception or was refused as
   side-effecting". The exception message never surfaces, making in-page debugging blind.
   Distinguish refusal from throw, and include a bounded, sanitized exception summary.
2. **Grouped-constraint schema errors do not name the missing field.** Temporal tools
   (`query_browser_events`, `list_source_frames`, `fetch_source_frames`) effectively
   require `focus_times: []` even when empty; omitting it fails with "tool arguments do
   not match the advertised input schema at $" — a root-path error with no field hint.
   Either make empty collections optional in the wire contract or report the specific
   unmatched constraint element.
3. **Fresh-session navigation failure lacks recovery guidance.** A managed session started
   without `initial_url` has zero pages; `navigate_page` then fails with "selected browser
   page was not found" — no recovery action ("create a page first with create_page") and,
   unlike other failures, no diagnostics block.
4. **Clipboard failure cause is opaque.** "browser denied or did not complete the
   clipboard request" does not distinguish focus loss vs. permission vs. insecure context,
   while the skill instructs the agent to "correct that browser state" — impossible
   without knowing which state.

## Simplification opportunity

All four are projection-boundary fixes: the underlying operations already know the
distinguishing cause (CDP exception details, serde path, empty-session state, clipboard
API error class). Route the existing cause through the validated-wire-contract error shape
instead of adding new error taxonomies; keep privacy bounds (sanitize exception text to a
bounded length, never page content).
