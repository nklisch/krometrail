---
name: krometrail
description: >
  Operate a local Chromium browser with Krometrail and inspect current or retained visual evidence.
  Use for browser navigation and interaction, structured page targeting, screenshots, viewport
  emulation, transient visual debugging, temporal storyboards, source frames, and browser events.
---

# Use Krometrail browser evidence

Follow the user's debugging direction. Use the MCP tool descriptions as the authority for arguments
and response schemas; a harness may prefix tool names with `krometrail`.

If tools are absent, read [setup and activation](references/setup.md). The plugin manages its exact
release binary. Do not install a separate binary merely because this skill loaded.

## Use the cheapest sufficient evidence

1. Trust the live evidence returned by a successful state-changing operation for immediate
   confirmation. Do not take a redundant screenshot after every click, fill, key press, navigation,
   scroll, or viewport change.
2. Use `observe_live` when an explicit fresh current-state observation is needed, or a narrower
   `inspect_page`, `snapshot_page`, or `take_screenshot` when only one part is needed.
3. Use `temporal_debug_bundle` or retained source evidence only for history, transient behavior,
   ambiguity, or a claim that the final state cannot establish.

These are distinct facts: post-operation evidence describes the operation's observation point;
`observe_live` describes a separately requested current point; retained source frames describe what
continuous capture stored over an interval. A successful action can still report degraded live
observation or retained-capture health. State those limits instead of treating them as action failure.

## Operate and target

To keep a visible managed Chrome window available for the user to watch without Krometrail
switching foreground tabs, start it with:

```json
{"focus":"preserve"}
```

The initial OS process launch may still surface Chrome. After launch, stay on one Chrome-visible tab
when possible: visible-page interactions do not activate the browser, while pointer work on a hidden
tab fails as `target_hidden` without stealing focus. `create_page` creates a background tab. Both it
and `select_page` change Krometrail's logical selection without switching Chrome's visible tab in
preserve mode. Omit `focus`
or use `{"focus":"foreground"}` when the user wants automatic tab switching.

- Lifecycle/pages: `start_browser`, `attach_browser`, `browser_status`, `stop_browser`, `list_pages`,
  `create_page`, `select_page`, `close_page`
- Current state: `inspect_page`, `snapshot_page`, `take_screenshot`, `observe_live`, `evaluate_page`
- Navigation: `navigate_page`, `reload_page`, `go_back`, `go_forward`
- Interaction: `click`, `fill`, `press_keys`, `select_option`, `hover`, `drag`, `scroll`,
  `upload_files`, `handle_dialog`, `wait`, `batch`
- Responsive state: `set_viewport` applies or clears explicit target-scoped metrics

Page-scoped requests default to the selected page unless their schema requires an explicit target.
Prefer an actionable reference returned by `snapshot_page`. Same-document snapshots preserve stable
node identities when possible; navigation, document replacement, target reattachment, or node removal
can make a reference stale. On `stale_reference`, request a fresh snapshot and retry once.

Valid locator shapes include:

```json
{"locator":{"kind":"element","value":{"kind":"css_selector","value":"button[data-testid='save']"}}}
```

```json
{"locator":{"kind":"element","value":{"kind":"reference","value":{"target_id":"<returned-target-id>","generation":1,"node_id":7}}}}
```

Copy all reference fields from one returned snapshot node; never invent or mix them. Krometrail
scrolls element targets into view before pointer dispatch. If a hidden/background page cannot be
activated, select or foreground it and follow the returned recovery guidance.

`fill` defaults to replace mode. Use `Meta` for Command shortcuts and `Control` for Control shortcuts;
Krometrail dispatches modifier chords without inserting their printable character. Use named keys such
as `Enter` for activation.

For text waits, omitting `locator` scopes the match to the full document-body text. `match_mode:
"exact"` compares the complete text in that scope; use a locator for exact element text or
`match_mode: "contains"` for an unscoped substring.

Apply an explicit responsive viewport with:

```json
{"viewport":{"mode":"override","metrics":{"width":390,"height":844,"device_scale_factor":3.0,"mobile":true,"touch":true}}}
```

Clear it with `{"viewport":{"mode":"clear"}}`. A geometry change creates a new visual epoch; do not
compare pixels across incompatible epochs without declared normalization.

For `batch`, each step is `{"operation":"<standalone tool name>","request":{...}}`; the request object
uses the same arguments advertised by that standalone operation. For example:

```json
{"steps":[{"operation":"fill","request":{"locator":{"kind":"element","value":{"kind":"css_selector","value":"#query"}},"value":"krometrail"}},{"operation":"press_keys","request":{"locator":{"kind":"element","value":{"kind":"css_selector","value":"#query"}},"keys":["Enter"],"wait_for_navigation":true}}],"timeout":5000}
```

The outer page target applies by default; do not nest `batch` or include browser-lifecycle operations
as steps.

## Inspect retained time only when needed

- Compact interval: `temporal_debug_bundle`
- Artifacts: `generate_artifacts`, `generate_region_filmstrip`
- Optional video: use `generate_temporal_video` only when it is advertised and a still artifact or
  source-frame read cannot answer the interval question economically. `real_time` preserves bounded
  relative timing; `model_optimized` may hold meaningful states and gap slates longer. Treat those
  presentation holds as declared provenance, not additional observed time.
- Source detail: `list_source_frames`, `fetch_source_frames`
- Full reads: returned `krometrail://evidence/...` resource links. For a compact bundle artifact,
  read its `manifest_uri` when the claim needs the full ordered source IDs, parameters, gaps, or
  normalization provenance; read the adjacent artifact URI when the rendered image is needed. Video
  results likewise keep MP4 bytes and the complete presentation manifest behind their returned local
  resource links; do not upload either unless the user explicitly authorizes a separate workflow.
- Browser context: `query_browser_events`
- Retention: `pin_resolved_range`, `query_pin_state`, `unpin_resolved_range`

Before relying on history, check `browser_status` capture state or the operation's capture warning.
Capture failure means current control may still work while new retained frames are unavailable. Dark,
partial, or otherwise degraded post-operation screenshots should be retried with `observe_live` after
the reported compositor boundary; if still degraded, use the returned diagnostic correlation rather
than guessing from the image.

Read [visual evidence semantics](references/evidence.md) before making a strong claim from a derived
artifact or an interval with capture gaps.

## Collect targeted diagnostics

Failed or degraded responses can include `diagnostics.correlation_id` and `diagnostics.log_path`.
Use that exact private path from any working directory. Search only for the correlation identifier,
with a few surrounding lines if required; never read, paste, or attach the whole log.

```bash
rg -n -C 3 --fixed-strings '<correlation-id>' '<diagnostics.log_path>'
```

Report only bounded event names, stable error codes, failure stages, route/outcome, timestamps, and
the correlation identifier. Exclude browser content, form values, screenshots, raw CDP traffic,
secrets, tokens, cookies, headers, and unredacted URLs. Use `$report-krometrail-issue` when the user
wants a GitHub report prepared.

## Interpret conservatively

- Source frames are authoritative only within reported cadence, retention, and known capture gaps.
- Derived artifacts are deterministic but lossy and do not diagnose cause.
- Difference and motion measurements describe observed pixels, not whether behavior is defective.
- Browser events correlate with visual change; they do not prove causation.
- Follow provenance to the resolved range, ordered frames, omissions, gaps, parameters, and epochs.

When Krometrail launched the browser for the task, stop it when finished unless the user wants it left
running.
