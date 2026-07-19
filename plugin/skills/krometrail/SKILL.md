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

Krometrail defaults to concise, action-centric structured responses without inline image bytes. Omit
`response` for normal navigation and interaction. The returned flat `targets` are ranked for immediate
action and each contains one complete, copyable generation-scoped reference.

Request broader bounded semantic/page context only when concise output cannot answer the question:

```json
{"response":{"detail":"expanded"}}
```

Request the complete acquired structures only when their full rows or provenance are genuinely needed:

```json
{"response":{"detail":"full"}}
```

Pixels are independent of structured detail. Add `"inline_images":true` at any level only when the
current image is needed in the immediate response:

```json
{"response":{"detail":"expanded","inline_images":true}}
```

Detail never changes action outcome, interaction identity, warnings, retained capture, range handles,
or canonical resource identities. Concise and expanded snapshot omission counts distinguish source
omissions from presentation omissions; neither projection pretends to be a complete snapshot tree.

1. Trust the live evidence returned by a successful state-changing operation for immediate
   confirmation. Its default concise response retains screenshot availability metadata without embedding
   image bytes. Request an inline image on the original action only when pixels are needed; do not take a
   redundant screenshot after every click, fill, key press, navigation, scroll, or viewport change.
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
preserve mode. Use `activate_page {}` for a deliberate one-shot foreground of the logical selection,
or pass its optional `target` to foreground one named page. It waits for visible state and returns
live evidence, but does not change either the selected page or the immutable focus policy. Omit `focus`
or use `{"focus":"foreground"}` when the user wants automatic tab switching.

- Lifecycle/pages: `list_managed_profiles`, `start_browser`, `attach_browser`, `browser_status`,
  `stop_browser`, `list_pages`, `list_page_contexts`, `wait_for_page`, `create_page`, `select_page`,
  `activate_page`, `close_page`
- Current state: `inspect_page`, `query_page`, `snapshot_page`, `take_screenshot`, `observe_live`, `evaluate_page`
- Context detail: `list_frames`, `list_page_assets`
- Navigation: `navigate_page`, `reload_page`, `go_back`, `go_forward`
- Interaction: `click`, `fill`, `press_keys`, `select_option`, `hover`, `drag`, `scroll`,
  `upload_files`, `handle_dialog`, `wait`, `batch`
- Responsive state: `set_viewport` applies ergonomic presets or custom target-scoped metrics, or restores browser defaults
- Explicit local I/O: `read_clipboard`, `write_clipboard`, `list_downloads`, `wait_for_download`, `cancel_download`

Clipboard calls are explicit, managed-session only, bounded text operations. Krometrail preserves
focus and Chrome permission policy: it does not activate a page or grant clipboard permission. If a
call reports focus, secure-context, API, or permission failure, correct that browser state instead of
trying to bypass it. Clipboard content belongs only in the explicit request/result; do not copy it
into issue reports or diagnostics.

For a download-triggering action, call `list_downloads {}` first and keep its `cursor`, then trigger
the action and call `wait_for_download` with `after` set to that cursor. This avoids racing a fast
download. Expand to `list_downloads` again when reconciling timeout or cancellation. Read a completed
download through its returned `krometrail://local/...` resource URI. These bounded bytes remain local
and exist only while the managed browser session is active; `stop_browser`, session loss, or MCP
restart invalidates the link. Attached browser sessions do not expose clipboard or download authority.

`browser_status {}` returns capture health, loss, retention pressure, cadence, selection, and page count.
Use `{"response":{"detail":"expanded"}}` to add page entries or `full` only for the complete acquired status.

Page-scoped requests default to the selected page unless their schema requires an explicit target.
Prefer `query_page` for routine targeting by accessible role/name, label text, rendered text, or
`data-testid`. The ergonomic default is exact, case-insensitive normalized text with at most 20
matches; request `contains`, case sensitivity, or a larger bounded limit only when needed. Branch on
the explicit `outcome`: proceed only with `unique`, narrow `ambiguous` or `truncated` queries, and
revise `no_match` queries. A descendant `scope` must be an exact current reference and excludes the
scope node itself. The default document is the current main document. When `list_frames` reports a
qualified same-origin/same-process frame, pass its complete frame reference as the `document` scope.
Refresh after frame navigation. Cross-origin, out-of-process, stale, or indeterminate frame scope
fails explicitly; never retry it against main-document coordinates.

For an unnamed control whose visible identity is text in its surrounding row or card, add
`container_text` to a role query. Krometrail qualifies the control against the nearest matching
ancestor's rendered text; it does not use page-wide text or geometric proximity. For example:

```json
{"query":{"kind":"role","role":"checkbox","container_text":{"value":"Buy milk","mode":"exact","case_sensitive":false}}}
```

Read [browser contexts and assets](references/browser-contexts.md) before work that reuses a named
profile, opens popups, enters frames, or diagnoses resource loading.

`query_page` returns exact generation-scoped references rather than persistent locators. Copy the
unique reference into an existing mutation tool; the action does not reevaluate the semantic query.
Prefer an actionable reference returned by the default concise `snapshot_page` response when examining
page structure or when semantic matching is insufficient. Request expanded context when surrounding
semantics matter, or full only for the complete acquired snapshot. Same-document snapshots preserve stable
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
activated automatically, call `activate_page` only when foregrounding is intended, then retry with a
fresh reference if the document changed. A `target_hidden` activation failure authorizes no input;
follow its returned recovery guidance.

`fill` defaults to replace mode. Use `Meta` for Command shortcuts and `Control` for Control shortcuts;
Krometrail dispatches modifier chords without inserting their printable character. Use named keys such
as `Enter` for activation.

For text waits, omitting `locator` scopes the match to the full document-body text. `match_mode:
"exact"` compares the complete text in that scope; use a locator for exact element text or
`match_mode: "contains"` for an unscoped substring.

For routine CSS breakpoint/layout testing, start with the smallest responsive preset and expand only
when the task needs a wider surface:

```json
{"viewport":{"mode":"preset","preset":"responsive_small"}}
```

The responsive presets are `responsive_small`, `responsive_tablet`, and `responsive_desktop`; they
change CSS geometry without mobile or touch emulation. Use `mobile_phone` or `mobile_tablet` only when
testing mobile-layout/touch behavior. Mobile presets do not emulate a mobile user agent or claim full
device fidelity. Use custom metrics only for bespoke geometry:

```json
{"viewport":{"mode":"override","metrics":{"width":512,"height":768,"device_scale_factor":1.0,"mobile":false,"touch":false}}}
```

The result reports the materialized metrics, responsive/mobile intent, independently observed visual
and layout geometry, and at most one guidance item. Responsive desktop presets acknowledge the declared
layout geometry; their visual content width may be smaller when Chrome reserves a scrollbar. Mobile
presets acknowledge the visual viewport because page scale and viewport metadata govern that surface.
A layout-mismatch guidance item describes page layout rather than an application failure; the specific
missing-viewport-metadata guidance suggests adding page viewport metadata or using a responsive preset
for CSS-breakpoint testing. Clear with
`{"viewport":{"mode":"clear"}}` to restore browser defaults. A geometry change creates a new visual
epoch; do not compare pixels across incompatible epochs without declared normalization.

For `batch`, each step is `{"operation":"<standalone tool name>","request":{...}}`; the request object
uses the same arguments advertised by that standalone operation. For example:

```json
{"steps":[{"operation":"fill","request":{"locator":{"kind":"element","value":{"kind":"css_selector","value":"#query"}},"value":"krometrail"}},{"operation":"press_keys","request":{"locator":{"kind":"element","value":{"kind":"css_selector","value":"#query"}},"keys":["Enter"],"wait_for_navigation":true}}],"timeout":5000}
```

The outer page target applies by default. Response detail is also outer-batch only: nested step
`request` objects cannot contain `response`. Do not nest `batch` or include browser-lifecycle operations
as steps.

## Inspect retained time only when needed

- Compact interval: `temporal_debug_bundle`
- Artifacts: `generate_artifacts`, `generate_region_filmstrip`
- Optional video: use `generate_temporal_video` only when it is advertised and a still artifact or
  source-frame read cannot answer the interval question economically. `real_time` preserves bounded
  relative timing; `model_optimized` may hold meaningful states and gap slates longer. Treat those
  presentation holds as declared provenance, not additional observed time. When the user requests a
  video, or video materially helps a video-capable model inspect the behavior, generate it without an
  extra confirmation step.
- Source detail: `list_source_frames`, `fetch_source_frames`
- Full reads: returned `krometrail://evidence/...` resource links. For a compact bundle artifact,
  read its `manifest_uri` when the claim needs the full ordered source IDs, parameters, gaps, or
  normalization provenance; read the adjacent artifact URI when the rendered image is needed. Video
  results return local MP4 and manifest resource links. Read the MP4 when the host supports video input;
  read the manifest when exact presentation provenance matters.
- Browser context: `query_browser_events`
- Retention: `pin_resolved_range`, `query_pin_state`, `unpin_resolved_range`

Keep the `range_handle` returned by `temporal_debug_bundle`. For follow-up artifact, region,
source-frame, browser-event, pin-state, and advertised video tools, pass that handle instead of
copying the full resolved range:

```json
{"range_handle":"<returned-range-handle>"}
```

Those tools require exactly one of `range_handle` or `range`; keep every other argument required by
the tool schema. A handle is an immutable process-local convenience for the exact validated range.
It survives `stop_browser` while its retained frames remain available, but it does not survive a
plugin/MCP restart or session-data deletion. On `evidence_invalidated`, run
`temporal_debug_bundle` again and use the new handle. Copy the full `range` only when crossing MCP
process boundaries or preserving an exact external record. The handle never replaces artifact/video
manifest provenance, ordered frame IDs, gap checks, or canonical evidence resource links.

The temporal bundle defaults artifact work to the visual epoch containing its effective anchor while retaining
the complete resolved range and gaps. Use `{"epochs":"all"}` only when the investigation depends on a viewport,
orientation, or device-scale transition; direct `generate_artifacts` requests remain all-epoch. The default
response is a concise resource-and-provenance index with one primary compact handle/resource, exact
epoch/outcome/resource omission counts, and no inline image bytes. Add
`{"response":{"inline_images":true}}` when the primary orientation/storyboard image should be embedded
immediately, use `expanded` for compact handles/resources for every generated outcome, or use `full` when the
complete acquired generator, frame, and provenance structures are genuinely needed in the tool result. Otherwise follow the
returned canonical resource links for the exact artifact needed.

Video and manifest resources stay local by default. Reading them through Krometrail or handing a
returned local resource to the active model host is normal tool use; do not upload or forward them
through a separate external workflow unless the user explicitly authorizes it. Prefer a smaller
still artifact or targeted source read when it can answer the question, but do not let that preference
block a requested or useful clip. Treat manifests as potentially sensitive evidence because they
identify source frames, timing, gaps, and browsing-session artifacts. When reporting diagnostics,
never expose frame pixels, page content, raw FFmpeg input or stderr, or local executable paths.

Before relying on history, check `browser_status` capture state or the operation's capture warning.
Capture failure means current control may still work while new retained frames are unavailable. Dark,
partial, or otherwise degraded post-operation screenshots should be retried with `observe_live` after
the reported compositor boundary; if still degraded, use the returned diagnostic correlation rather
than guessing from the image.

Read [visual evidence semantics](references/evidence.md) before making a strong claim from a derived
artifact or an interval with capture gaps.

## Collect targeted diagnostics

Failed or degraded responses always include `diagnostics.correlation_id`; `diagnostics.log_path` is present
when logging was configured.
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
