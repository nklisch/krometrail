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
updated: 2026-09-30
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

**Diagnose which stage fails first** (revised after the Astra design review
of `ebef666b`). Two credible paths make a page unlistable, and the code alone
cannot tell which one the incidents took:

1. `attach` → initial visibility probe → `InitialVisibilityProbeFailed`,
   which detaches and terminally fails the target. Note that this input also
   covers mandatory session-domain setup failing before the visibility query.
2. A successful probe → capture geometry or screencast startup →
   `CaptureStartFailed`, which detaches and terminally fails the target.
   `create_page` waits for that whole effect queue before reporting an attach
   failure. Hidden pages skip capture startup, which fits the reference
   host's "reattach worked and saw `hidden`", and it conflicts with the
   capture/control isolation contract in `docs/ARCHITECTURE.md`.

Reproduce first on the lane's candidate, with failures classified by stage
(attach, domain setup, visibility, geometry, screencast). Record which stage
fails, and fix that stage; do not assume path 1.

**Required behavior.**

- A target that attached and completed its mandatory domain setup is never
  terminally failed only because its visibility could not be observed, or
  because capture could not start. It stays listed and selectable, and it
  answers control operations. Its visibility is `unknown` until actually
  observed; `hidden` requires an observation. Its capture binding records
  that capture is unavailable, instead of failing the target, as the
  capture/control isolation contract requires.
- A real attach failure or mandatory domain-setup failure still fails the
  target. Split the reducer input so the two cases cannot be confused.
- Readiness accepts an attached, initialized target whose visibility is
  unknown. The visible-only capture gate stays: capture starts only after
  observed visibility.
- Recovery trigger: explicit activation (which already commits visibility),
  plus one bounded visibility re-probe when `list_pages`, `select_page`, or
  a page operation needs the target and its visibility is unknown. A capture
  start that failed is retried when a later observation shows the target
  visible. Reconnect uses the same policy instead of failing the
  replacement connection on a visibility-probe failure. Preserve target
  identity, attachment-generation fencing, and existing selection semantics.
- `create_page` returns the created page once it has attached and completed
  domain setup, even while its visibility or capture is pending.
- Existing selection-recovery work (the page-selection-recovery epic) is
  related but not absorbed; note any overlap.

**Verification.**

- Reducer tests, plus the existing scripted-CDP transport (failure injection
  and held commands) covering: first attach, list, and `create_page` with a
  slow or failing visibility probe; a failing capture start; later recovery
  through re-probe and activation; no capture before observed visibility; a
  real attach or domain-setup failure staying a failure (negative test); and
  reconnect keeping identity with unknown visibility. Update the existing
  test that rejects unknown visibility at readiness to the new contract.
- A real-browser check with the lane's candidate binary: Chrome with
  loopback remote debugging inside an NCU owned desktop that the lane creates
  and destroys by explicit ID. Attach, list the page, run `create_page`, and
  do one interaction, all on first attach, repeated several times.
- The same journey on a fresh Nobara workstation VM through
  `dave-and-nate-games/workstations` `tests/vm/ncu_desktops.py --krometrail
  <guest path>`, using the candidate.
- The repository's own test and qualification commands, and the release
  gates at release time.

**Release.** After review and acceptance, the owner releases through
`bun scripts/bump-version.ts patch` and the tag-driven `release.yml`, with the
user's standing agreement for this workstream, and closes #16 with the
evidence.

**Difficulty.** High on diagnosis, moderate on the change. Likely mistakes:
fixing the visibility path when capture startup is the real failure;
suppressing a failure without making the target recoverable; starting capture
on a target whose visibility is unknown; masking a real attach or setup
failure as success.

## Delivery notes

### Diagnosis (2026-09-30, before behavior changes)

Candidate built from lane HEAD with stage-only instrumentation, using
`CARGO_TARGET_DIR=~/.cache/dng-workstations/krometrail-target`. Fresh explicitly
owned NCU 0.4.0 desktop `desktop-1790748320571-3953145`, Google Chrome, private
profile, loopback port 0, synthetic HTTP fixture. Independent `/json/list`
reported the page and NCU observation recorded its window. First attach returned
ready/page_count 0; list returned []; create returned target_failed.

Stage evidence at 2026-09-30T06:05:34Z: attach succeeded, mandatory domain setup
succeeded, visibility probe succeeded, geometry failed. Screencast was not
attempted. This establishes design path 2: CaptureStartFailed terminally fails
an otherwise initialized controllable target. The exact viewport-decoding cause
is not yet established. Instrumentation initially emitted an extra visibility
false record for Attached inputs; that record is not a probe and is removed.
Evidence is local at `~/.cache/dng-workstations/attached-page-evidence/`:
`baseline.log`, `host-0.png`, and `data-baseline-0/diagnostics/krometrail.log`.
Attach correlation ac21fda6-1c77-4d8c-8593-c4944331041a; create correlation
149d4465-015d-4ca3-9846-ebb2a5bed6fd. Candidate MCP was 1.7.0. Exact desktop
destruction succeeded; no external browser or installed binary was used.

### Implemented behavior

- Split mandatory `DomainSetupFailed` from optional visibility-probe failure.
  Attach/setup failures remain terminal and release their exact flat sessions.
- Initialized targets with unknown visibility are ready, listed, selectable,
  and addressable. Unknown is never synthesized as hidden or visible. Capture
  failures retain the attachment and selection with `CaptureBinding::Unavailable`.
- Visibility probes have a 250 ms ceiling. List/select/page use re-probes unknown
  targets once; a listing shares one ceiling across its unknown targets.
  Reconnect applies the same policy, including a visibility timeout at the
  attempt deadline after mandatory setup completed. Cancellation still aborts.
- A later visibility observation (including unchanged visible state committed
  by activation) retries unavailable capture. Retry admits the same attachment
  generation only after stream admission excludes an active/concurrent start;
  older generations remain fenced and capture ordinals are preserved.
- `create_page` returns its initialized page despite unavailable visibility or
  capture. Stage-only initialization diagnostics contain no page content.
  Architecture documentation now states this supported contract.

### Verification results

All builds used `~/.cache/dng-workstations/krometrail-target`; installed binaries,
other browser sessions, and user configuration were untouched.

- Reducer and scripted-CDP tests: PASS. Covers unknown readiness/selection,
  no premature capture, held visibility during attach/list/create, failing
  visibility during creation, list/select/page-use recovery, geometry and
  screencast start failure, activation retry, mandatory-domain/real attach
  failure, and reconnect identity/generation with failed or timed-out visibility.
- `cargo fmt --all -- --check`: PASS.
- `bash scripts/check-wire-enum-schemas.sh`: PASS.
- `cargo check --workspace --all-targets --locked`: PASS.
- `cargo test --workspace --all-targets --locked`: PASS; repository opt-in real
  browser/manual benchmark tests retain their existing opt-in/ignored status.
- `rustup run 1.98.0 cargo-clippy clippy --workspace --all-targets --locked --
  -D warnings -A clippy::chunks_exact_to_as_chunks`: PASS.
- `cargo run -- --version`, `--help`, and `doctor`: PASS (1.7.0;
  discovery-only doctor found one installation).
- `bun install --frozen-lockfile` and `bun run docs:build`: PASS.
- Owned NCU desktop qualification: PASS on three fresh Chrome
  151.0.7922.137 desktops, first attach page_count 1, first list one page,
  fill succeeded and HTTP fixture independently confirmed the exact text,
  create with the fixture initial URL succeeded, subsequent list two pages.
  All three exact desktops were destroyed. `final-host.log` is the receipt.
  Three earlier candidate runs also passed (creation used default about:blank).
- Fresh Nobara VM: PASS through the read-only workstation harness at
  `dfeb1898e749a1f1cab64ab6d6999a2387dbea0d`, candidate copied to
  `/home/tester/journey/krometrail-candidate` via its existing `Guest.copy` seam.
  Original journey found one page on first attach and independently confirmed
  fill; extended guest-only fixture added create_page/about:blank and a two-page
  listing assertion. Both journeys passed cold boot/no graphical login, advancing
  NCU capture, input, SSH frontend reconnect, exact desktop destruction, and
  exact VM/volume cleanup. Original VM `dng-ncu-a2d9c377ed`; extended VM
  `dng-ncu-821e743b31`. No workstation repository files were edited.
  Evidence: `vm.log`, `vm-extended.log`, and harness directories
  `~/.cache/dng-workstations/ncu-desktops/journey-658a00907e/` and
  `journey-0c28d92374/` (including reports and synthetic frames).

### Remaining evidence and scope

Direct CDP in another exact owned desktop established Chrome's default
`navigator.maxTouchPoints = 10` alongside valid dimensions and scale, with and
without `throwOnSideEffect` (`geometry.log`). The existing no-override decoder
rejects this default capability as “browser did not clear touch emulation”.
This explains the geometry failure; capture/control isolation is fixed here,
while viewport/default-touch semantics are recorded separately in
`.work/backlog/default-touch-capture-geometry.md`. Continuous Krometrail recording
on this default-touch host is therefore not qualified by the control journey.

The related page-selection-recovery epic overlaps diagnostics and selection
recovery but remains outside this story. No release/version change, issue closure,
push, or tag was performed. Story remains implementing for the owner's Opus +
GPT-6 Astra checkpoint. Physical GPU monitorless behavior, macOS, Windows, and
release-time qualification remain unverified by this lane.

Final binary recheck after diagnostic cleanup: PASS on another fresh owned desktop
(`final-recheck.log`), first attach/list/fill/fixture-URL create/list/detach, with
independent fixture text confirmation and exact desktop destruction. Candidate
SHA-256: `6a0b8d1d4517252f527035692229becca3396cc3bf9331ecc7a5776316c53db2`.
