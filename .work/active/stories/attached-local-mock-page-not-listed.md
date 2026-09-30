---
id: attached-local-mock-page-not-listed
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-09
updated: 2026-09-29
---

# Attached browser reports no pages despite a visible local mock

During Orogen Ledge mock verification, `attach_browser {endpoint: "http://127.0.0.1:9438"}` succeeded with server_version 1.7.0, state ready, page_count 0 and selected_target_id null. A subsequent `list_pages {}` succeeded with an empty result. No diagnostic correlation or error was returned. Session: b63a8c1b-4a9d-4f31-88e8-b89cf4cfe4b0.

The explicitly owned NCU desktop had a visible Google Chrome page displaying a synthetic local HTML mock. Reading the same endpoint's `/json/list` independently returned that `type: page` target and a working page websocket. Direct browser inspection and screenshots worked through it. Expected: the existing visible page becomes available after attachment; observed: no page was listed. Root cause is unknown; this may be a file-URL discovery constraint or another attachment issue, not established by this bounded attempt.

At cleanup, `stop_browser {}` returned `invalid_lifecycle_transition`: “no browser session is active”, correlation `e092e95d-a33d-474f-a865-5ebd1964e8d6`. This agent did not stop the attached session between calls. Other MCP activity or frontend lifetime may explain the loss; no cause is established. The underlying Chrome remained available for the direct checks.

Workaround: direct CDP for browser mock assertions and rendered screenshots, NCU for native keyboard/mouse verification. Neither implies real-terminal qualification. No toolkit fix was attempted. The temporary desktop and its Chrome profile were destroyed after the original task.

## Recurrence during local WebM gallery review — 2026-09-11

Krometrail 1.7.0 attached to an explicitly owned Chrome endpoint on loopback port
19567. Initial attachment and repeated `list_pages {}` reported zero pages while
the native desktop displayed the local HTML. The endpoint's `/json/list` reported
two page targets with websocket URLs. `create_page` created a visible second tab
but returned `target_failed`: “created browser target could not be attached”,
correlation `d6358565-bc99-4bdf-ab2d-7b4056073225` at 00:53:15 UTC. Bounded
correlation-adjacent logs show `browser.target.attached` immediately after that
failed response; this is evidence of ordering, not a proven root cause.

A clean detach/reattach temporarily returned one page and allowed media-property
reads plus a click. Later selected-target identity changed without an explicit
select call; `activate_page` reported activation with unavailable observation,
then scroll failed `not_found`: “selected browser page was not found”, correlation
`faaebaca-b87e-426a-adc3-1043834584bb`; read-only evaluation returned the same error
under `b1edbfe7-8c09-4527-90cd-46100101ad30`. The two actual Chrome tabs remained
visible. Shared MCP frontend activity is a possible confounder; do not infer an
NCU or Chrome defect. The task continued using NCU observation/input and bounded
local media checks. No toolkit behavior was changed.

## Recurrence during NCU attachment qualification — 2026-09-12

Observed at 2026-09-12T21:34:14Z (HTTP fixture create failure; diagnostic timestamp),
reported at 2026-09-12T21:35:48Z. Krometrail MCP reported server_version 1.7.0;
installed NCU was 0.3.0 (retained daemon/worker build revision unknown), Chrome
151.0.7922.137. The desktop was newly created and claimed exclusively for this
qualification. Other Krometrail instances were running; interference is not ruled out.

NCU launched Chrome with its private profile, Wayland routing, disposable
`chromium_automation` preset, `--remote-debugging-address=127.0.0.1`, and
`--remote-debugging-port=0`. The browser WebSocket endpoint was read from that
profile's `DevToolsActivePort`. Attachment succeeded but returned page_count 0.
`create_page` failed for both a synthetic local file and a loopback HTTP fixture:
`target_failed: created browser target could not be attached`. Correlations:
`86cbf014-f9c5-4101-aa8e-e1e4b009d55f` (file) and
`0e4f306b-d47a-4230-89b0-817852be5e6f` (HTTP). Thus this recurrence is not limited
to file URLs. The HTTP failure's bounded adjacent diagnostics show
`browser.target.failed`, `browser.target.discovered`, the failed MCP response,
then `browser.target.attached` with a different target identity. This ordering
is evidence, not an established cause.

A single clean `stop_browser` (closure detached) and reattach returned two pages.
Read-only evaluation and live screenshot inspection succeeded; a Krometrail
button click changed the fixture label, independently confirmed in an NCU
screenshot. A later status returned a different selected target without an
explicit selection, `capture: []`, and no retained bounds. Browser control is
qualified through this workaround; continuous temporal recording is not proven.
Do not blindly recreate tabs after an attachment error: inspect existing browser
state first, and only detach a session known to belong to the current task.


## Recurrence during Orogen canonical mock review — 2026-09-13

Observed at 2026-09-13T15:59:58.642569Z (query failure diagnostic timestamp);
filed 2026-09-13T16:10:16+00:00. Krometrail MCP reported 1.7.0; installed NCU
reported 0.3.0. Browser-runner evidence from the same machine reports Chrome
151.0.7922.137; the NCU-launched browser version was not separately queried.

A new task-owned, claimed NCU desktop launched Chrome with a private disposable
profile, loopback remote debugging and a local synthetic HTML mock. Native
observation confirmed the rendered page. Attachment via that profile's browser
WebSocket endpoint succeeded with page_count 0 and no selected target;
list_pages remained empty. query_page failed not_found, “selected browser page
was not found”, correlation 8bd558d4-e30a-41ed-b8f7-09912d15322e. The same
endpoint's /json/list independently reported the visible type:page target.

One clean detach/reattach did not recover discovery. The second configured MCP
frontend reproduced zero pages against the same owned endpoint and also
reported 1.7.0. Other unrelated browser-tool instances existed; no interference
or root cause is established. No additional tabs were created as retries.
Continue the original task using native NCU interaction and the existing mock
verification harness; no toolkit fix is authorized by this report.

## Recurrence during Orogen wave-3 desktop capture work — 2026-09-18

Observed 2026-09-18, roughly 17:45–18:05 MDT (UTC-6). Krometrail MCP
server_version 1.7.0; Chrome 151.0.7922.137 (from the endpoint's own
`/json/version`); NCU 0.3.0. A task-owned claimed NCU desktop (1440×900)
launched Chrome with the private profile, `--remote-debugging-address=127.0.0.1
--remote-debugging-port=0`, a loopback HTTP fixture, and `chromium_automation`.

Attachment to the endpoint from `DevToolsActivePort` succeeded repeatedly
(state ready) but reported page_count 0 with no selected target across three
clean detach/reattach cycles spread over several minutes, while the same
endpoint's `/json/list` showed the visible `type: page` targets with valid
websocket URLs. `create_page` with a loopback URL failed
`target_failed: created browser target could not be attached`, correlation `c9c7feb4-4a1d-454f-b951-e3f1ddd86ddf`. Earlier in the
day, on the same machine and versions, the very first attach of a different
owned desktop returned one page after a single reattach, so the recovery is
intermittent rather than deterministic. Task was completed through in-fixture
click drivers and component tests; no toolkit fix attempted.

## Recurrence on workstation VMs and the reference host (2026-09-29)

GitHub issue nklisch/krometrail#16. With Krometrail 1.7.0, a newly launched
Chrome 151.0.7922.137 (reference host) or Nobara Chromium 151.0.7922.173
(fresh Nobara 44 VM) in an NCU owned desktop behaves the same way: the first
`attach_browser` returns page_count 0, `list_pages` returns [], and
`create_page` fails `target_failed`, while `/json/list` shows the page. On the
reference host, a detach and reattach listed the page with lifecycle and
visibility `hidden`. On the VM, the reattach also listed nothing. This blocks
browser automation on the Dave and Nate Games workstation fleet.

## Design

Owner design (Opus, main session of the workstation run), for GPT-6.1 Sol to
implement and Opus + GPT-6 Astra to review. The user asked for this fix as
its own workstream on 2026-09-29.

**Leading hypothesis, to confirm first.** `reconcile_one` issues `Attach` for
each recordable page, `attach` probes visibility, and a failed initial
visibility probe (`initial_visibility_probe_failed`) detaches and fails that
target, so it never becomes listable. In an NCU owned desktop the Chrome
window is on a headless KWin output. Right after launch, and on slower
software-rendered VMs, the page's visibility or lifecycle query can fail or
time out. That explains all three symptoms: zero pages at first attach,
`create_page` failing at its own attach, and a reattach that sometimes
succeeds (seeing `hidden`). Confirm or refute this from Krometrail's own
diagnostics and correlation-adjacent log entries before changing behavior.
Record the actual cause.

**Required behavior.** A visible or hidden page in an attached browser is
listed after attach. A page that cannot yet report visibility stays listed
with visibility `unknown` or `hidden`, and it is retried or refreshed on
later observation or activation instead of failing permanently.
`create_page` returns the page it created. Recording and capture still
start only when the existing rules allow it: this changes discovery and
listing, not the capture boundary. Existing selection-recovery work
(epic-a-grade-reliability-page-selection-recovery) is related but not
absorbed; note any overlap rather than expanding into it.

**Verification.** Reducer tests for a failed or slow initial visibility probe
leaving a listable target, and for recovering it later. A real-browser check
with the lane's candidate binary: launch Chrome with loopback remote
debugging inside an NCU owned desktop created and destroyed by the lane
(explicit ID), attach, list the page, `create_page`, and one interaction, all
on first attach, repeated several times. The same journey on a fresh Nobara
workstation VM through `dave-and-nate-games/workstations`
`tests/vm/ncu_desktops.py --krometrail <guest path>` with the candidate. Then
the repository's own test and qualification commands, and the release gates
(security, tests, cruft, docs, patterns) at release time.

**Release.** After review and acceptance, the owner releases through
`bun scripts/bump-version.ts patch` and the tag-driven `release.yml`, with the
user's standing agreement for this workstream, and closes #16 with the
evidence.

**Difficulty.** High on diagnosis, moderate on the change. Likely mistakes:
suppressing the failure without making the target recoverable; starting
capture on a target whose visibility is unknown; masking a real
target-attach failure as success.
