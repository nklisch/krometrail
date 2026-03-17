## Browser Lens Investigation Workflow

When the user mentions a browser issue, bug, or unexpected behavior:

1. **Find the session:**
   `krometrail chrome sessions --has-markers`
   Look for sessions with markers near the reported time.

2. **Get the overview:**
   `krometrail chrome overview <session_id> --around-marker M1`
   Understand the navigation path, errors, and markers.

3. **Search for errors:**
   `krometrail chrome search <session_id> --status-codes 400,422,500`
   Find network failures. Also try:
   `krometrail chrome search <session_id> --query "validation error"`

4. **Inspect the problem moment:**
   `krometrail chrome inspect <session_id> --marker M1 --include network_body,console_context`
   Get full request/response bodies, console output, and surrounding events.

5. **Compare before and after:**
   `krometrail chrome diff <session_id> --before <load_time> --after <error_time> --include form_state`
   See what changed between page load and the error.

6. **Generate reproduction artifacts:**
   `krometrail chrome replay-context <session_id> --around-marker M1 --format reproduction_steps`
   Or generate a test:
   `krometrail chrome replay-context <session_id> --around-marker M1 --format test_scaffold --framework playwright`

### Alternative: drive the browser with batch steps, then investigate

Instead of asking the user to reproduce a bug manually, drive the browser yourself:

```
chrome_start(url: 'http://localhost:3000', profile: 'krometrail')
chrome_run_steps({ steps: [
  { action: "navigate", url: "/checkout" },
  { action: "fill", selector: "#card", value: "4111111111111111" },
  { action: "submit", selector: "#payment-form" },
  { action: "wait_for", selector: ".error", timeout: 5000 }
]})
chrome_stop()

# Or reload and clear the buffer for a fresh start without restarting:
chrome_refresh()

krometrail chrome overview <session_id>
```

Each step is auto-marked (`step:1:navigate:/checkout`, etc.) so you can search and diff around any step.

### Tips
- Markers placed by the user are labeled [user]. Auto-detected markers are [auto]. Step markers are labeled `step:N:action:detail`.
- Use `--token-budget` to control response size (default: 3000 tokens for overview, 2000 for search).
- Event IDs from search results can be used with `--event <id>` in inspect.
- HAR export: `krometrail chrome export <session_id> --format har --output debug.har`
- **Screenshot shows empty/broken page but network returned 200?** Check the response body with `inspect --event <id> --include network_body` — it may contain streaming errors. Also check server logs.
- **Use `reload` not `navigate` for a full page refresh** — navigating to the same URL may hit SPA client-side routing cache.
- **Use `press_key` to submit forms without a submit button** — `{ action: "press_key", key: "Enter", selector: "#input" }`. Common for chat UIs and search bars.
