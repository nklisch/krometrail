---
name: krometrail
description: >
  Use Krometrail when operating a local Chromium browser, inspecting current page state, debugging
  transient visual behavior, or examining what happened over time through screenshots, structured
  snapshots, temporal storyboards, difference maps, filmstrips, motion history, source frames, and
  browser events.
---

# Use Krometrail browser evidence

## Follow the user's direction

Krometrail supplies browser control and visual evidence; it does not prescribe a debugging process.
Start from the user's question and stated approach. Select only the evidence that can answer that
question, and do not force every investigation through the same sequence.

Use the MCP tool descriptions as the authority for current arguments and response schemas. Tool names
may receive a harness-specific `krometrail` prefix.

If the Krometrail tools are unavailable, or the MCP server reports that the executable is missing,
read [setup and activation](references/setup.md). Plugin installation and binary installation are
separate facts. Never download or install the executable merely because this skill loaded.

## Choose evidence by question

| Question | Useful Krometrail evidence |
|---|---|
| What is on the page now? | `observe_live` for page state + snapshot + screenshot, or a narrower `inspect_page`, `snapshot_page`, or `take_screenshot` |
| What can I safely target? | `snapshot_page`; use its current actionable references rather than guessing selectors |
| What happened around an action or interval? | `temporal_debug_bundle` for a compact, source-linked combination of visual and browser-event evidence |
| Did a transient, reversal, or intermediate state occur? | storyboard artifacts, then selected or exact source frames when the sequence needs confirmation |
| Where did pixels change? | difference map; it shows location and accumulation, not cause |
| How did one small area evolve? | `generate_region_filmstrip` with a declared fixed region |
| Was there repeated movement or broad oscillation? | motion-history artifact; it shows accumulated recent change, not inherent direction |
| What exactly was retained? | `list_source_frames`, `fetch_source_frames`, or `retrieve_source_frame` |
| Did console, exception, network, or navigation evidence coincide? | compact events in a debug bundle or detailed `query_browser_events` |
| Will evidence need to survive eviction during a longer investigation? | `pin_resolved_range`, `query_pin_state`, then `unpin_resolved_range` when protection is no longer needed |

These are choices, not required stages. A current screenshot may be sufficient for a current-state
question. A source frame may be better than a generated artifact when one exact moment matters.

## MCP capability map

### Browser lifecycle and pages

- `start_browser`, `attach_browser`, `browser_status`, `stop_browser`
- `list_pages`, `create_page`, `select_page`, `close_page`

`start_browser` launches a controlled local Chromium browser with a managed profile. `attach_browser`
uses an explicitly configured local endpoint. A session has one immutable `every_nth_frame` capture
stride; it is relative sampling, not an exact frames-per-second promise.

### Observe and operate

- Current evidence: `inspect_page`, `snapshot_page`, `take_screenshot`, `observe_live`
- Read-only page code: `evaluate_page`
- Navigation: `navigate_page`, `reload_page`, `go_back`, `go_forward`
- Interaction: `click`, `fill`, `press_keys`, `select_option`, `hover`, `drag`, `scroll`,
  `upload_files`, `handle_dialog`
- Coordination: `wait`, `batch`

State-changing operations return live evidence or an explicit observation failure. Snapshot references
are generation-scoped: after navigation or DOM replacement, take a new snapshot rather than reusing a
stale reference. CSS selectors and coordinates are fallback targeting forms for cases where structured
references cannot represent the surface.

### Inspect time

- Compact investigation: `temporal_debug_bundle`
- Artifact generation/read: `generate_artifacts`, `generate_region_filmstrip`, `retrieve_artifact`
- Source detail: `list_source_frames`, `fetch_source_frames`, `retrieve_source_frame`
- Retention: `pin_resolved_range`, `unpin_resolved_range`, `query_pin_state`
- Correlated context: `query_browser_events`

Responses can include a context-sized image plus MCP resource links for full-resolution artifacts,
manifests, and source frames. Follow those links when the compact response is insufficient; do not
assume a thumbnail contains fine detail.

Read [visual evidence semantics](references/evidence.md) before making a strong claim from a derived
artifact or from an interval with capture warnings.

## Interpret evidence conservatively

- Source frames are the authoritative visual record, within the reported cadence and known gaps.
- Storyboards, difference maps, filmstrips, motion history, and composites are deterministic but lossy
  source-derived views.
- Measurements describe observed change. They do not prove that motion is wrong, a reversal occurred,
  one event caused another, or a particular code path is responsible.
- A capture gap means unseen behavior may have occurred. Do not describe the interval as continuously
  stable across it.
- Use provenance to identify the resolved time range, ordered source frames, omitted frames, gaps,
  normalization, parameters, algorithm version, and output hash behind a visual claim.
- Browser-event evidence can correlate with visual change but is not causal proof.

## Example: investigate without turning the example into a rule

If a user says a card briefly moved backward after submission, one reasonable approach is to perform
or identify that interaction, inspect its `temporal_debug_bundle`, and then request the storyboard or
source frames around the suspected reversal. A region filmstrip may help if only the card matters;
`query_browser_events` may help if the user suspects a request or exception. If the user instead asks
only for the current final state, a live observation can be enough.

When Krometrail launched the browser for the task, stop it when the work is finished unless the user
wants the controlled session left running.
