---
id: epic-state-aware-interaction-results-side-channel-outcomes
kind: feature
stage: review
tags: [agent-ux, browser]
parent: epic-state-aware-interaction-results
depends_on: [epic-state-aware-interaction-results-postcondition-core]
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Side-channel outcomes

## Brief

Extends the postcondition block with side-channel outcome facts so an
interaction that should open a page, start a download, or produce a clipboard
result can be verified from its own result (issue #14 findings #8 and #9).
Three surfaces, each with a mapped gap:

- **New page/popup facts.** The target supervisor already assigns a monotonic
  `PageSequence` on adoption with opener relationships; the page cursor is
  deliberately never absent. A post-action page-cursor delta (new page adopted,
  opener matches the acting target) becomes an observed fact. Blocked
  `window.open` currently produces no signal at all — investigate whether a
  bounded negative/attempt signal is observable (e.g. a page-emitted open
  attempt event) and otherwise report the honest "no new page observed" fact.
- **Download facts.** The download authority is lazily activated and its
  inventory cursor is `Option` — absent until something is recorded — which is
  exactly the unusable state finding #9 hit (`list_downloads` empty with no
  cursor after an activation). Align the download cursor with the page-cursor
  "never absent" contract, resolve the lazy-activation interplay for
  interaction-time facts, and record a bounded outcome fact when download
  activity follows an interaction. A suppressed/never-begun download leaves no
  record today; design must decide what is honestly observable.
- **Clipboard facts.** Root-cause the finding #8 failure mode: a dispatch death
  classified as transport (`command_failed`) with no permission prompt and no
  way to distinguish product failure from browser limitation. Improve failure
  classification at the clipboard boundary and record explicit clipboard
  operations' outcomes as bounded facts.

Root-cause obligations from the epic body land here: both #8 and #9 may hide
concrete defects; reproduce with deterministic doubles and boundary fault
injection (layered-cdp-qualification) — the reporting surface must not paper
over a real bug.

## Advisory constraints (binding, from the epic's cross-model adjudication)

- **Supervisor serialization**: `Execute` is processed serially — queued
  `Input(TargetCreated)` events are handled only after the operation returns,
  so `page_contexts()` reflects pre-action state during interaction
  execution. Popup facts require a post-dispatch target-event reconciliation
  step (or assembly after target reduction); do not read the page cursor
  inside `execute_interaction_request_inner` and call it a post-state.
- **Clipboard scope narrowed**: enrich the explicit clipboard operations'
  records and fix failure classification (#8's `command_failed` ambiguity);
  no automatic clipboard probing after arbitrary clicks.
- **Download activation ordering**: the lazily-activated download authority
  must be subscribed/enabled before interaction dispatch or early downloads
  stay unobservable; cursor seeded like the page cursor (`next - 1`, never
  absent); `WaitForDownloadRequest.after` becomes required.
- **CDP signal qualification**: `Page.windowOpen` = bounded open-attempt fact
  (no blocked field); `Page.frameRequestedNavigation` with disposition
  `download` = download-attempt candidate; `Browser.downloadWillBegin`
  onward = lifecycle facts. There is no general suppressed-popup/download
  signal: report attempt/outcome/no-outcome-observed, never "blocked".
- **Bounded collections**: any per-interaction side-channel lists carry
  canonical caps and exact omission counts before serialization into the
  record (the record's byte discipline is enforced at construction).
- **Navigation-fact upgrade (advisory F5, accepted follow-up)**: one pre/post
  URL pair is a URL delta, not a navigation delta — it misses same-URL
  reloads and navigations that commit and return before observation. Back the
  navigation fact with an always-on, non-waiting main-frame
  `Page.frameNavigated` + `Page.navigatedWithinDocument` signal (cursor or
  drained receiver), keeping `url_changed` as a separate fact; record signal
  availability alongside.

## Epic context

- Parent epic: `epic-state-aware-interaction-results`
- Position in epic: consumer of `postcondition-core`'s block; producer of the
  side-channel facts `expectation-notes` reasons over.

## Simplification opportunity

- Reconcile the two cursor contracts: the page cursor is never absent by
  design, the download cursor is `Option`. One "cursor is never absent"
  contract deletes the absent-cursor special case and its recovery prose.
- Prefer facts derived from existing authorities (page inventory, download
  tracker, clipboard boundary) over any new parallel event stream.

## Foundation references

- `docs/SPEC.md` — Current-State Observation (side-channel postconditions),
  Browser Lifecycle (pages), Local Data (downloads/clipboard boundaries)
- `docs/ARCHITECTURE.md` — Target Lifecycle, MCP Boundary
- GitHub issue #14, findings #8 (`7809bc9c-230d-4674-a7ea-befd309d4b21`) and
  #9 (`a6e7a7bd-340c-4fc6-a922-feabcd61a64a`)

## Design decisions

Resolved with judgment under the active autopilot goal (all seams verified in
source before deciding):

- **Popup/download fact assembly lands in `execute_operation`
  (`crates/krometrail-cdp/src/session/operations.rs`), after
  `execute_operation_unfenced` returns and before `persist_result_evidence`** —
  the one session-layer point that owns `&mut SupervisorState`, the transport,
  and the mutable result ahead of evidence persistence. The post-state comes
  from a bounded pull-based target reconciliation (one `Target.getTargets` →
  `reduce(SupervisorInput::InitialTargets)` → `apply_effects`), the exact
  pattern `wait_for_page` already runs mid-`Execute` (operations.rs ~310-352).
  This satisfies the binding serialization constraint: queued
  `Input(TargetCreated)` events still sit behind the operation, but the pull
  observes the browser's authoritative inventory directly. The interaction
  path never reads the page cursor inside `execute_interaction_request_inner`
  and calls it post-state. Batch steps recurse through `execute_operation`
  (`control/batch.rs` ~359), so per-step enrichment and persistence are
  inherited with no batch-specific code.
- **Download authority activates eagerly at managed session start** (in
  `session/mod.rs` right after the authority is constructed, ~421-430, where
  the connection transport is in hand) rather than at first interaction.
  Activation failure stores the existing `unavailable` error and never fails
  session start; interactions then carry `downloads: None` and explicit
  download operations report the stored error. This deletes the
  `activated()` "call list_downloads before triggering a download" trap —
  the exact unusable state finding #9 hit — and makes the cursor available
  from session start. Chosen over first-interaction activation because it
  removes the lazy special case entirely (one contract) at the cost of ~3
  cheap CDP commands per managed session.
- **Attempt signals ride the existing per-target page-signal broadcast**
  (`events/domain.rs` pump + `PageSignalKind`), not a new event stream:
  `Page.frameNavigated`/`Page.navigatedWithinDocument` are promoted to
  always-installed operation signals (currently gated on the semantic
  pipeline, domain.rs ~221-229); `Page.windowOpen` and
  `Page.frameRequestedNavigation` are installed as signal-only sources
  (never normalized/persisted, install failure degrades silently without
  class-unavailability accounting). No `BrowserEventKind` additions, no
  wire-registry growth.
- **#8 root cause (recorded, fixed here)**: with cdpkit 0.4.0,
  `CdpError::Timeout` maps to `TransportError::CommandFailed`
  (`transport/cdpkit.rs` ~206) and browser command rejections map to
  `Protocol` — so the observed `command_failed` dispatch death is a
  **command timeout**: the `navigator.clipboard.readText()` promise never
  settled (consistent with a pending/suppressed permission decision or an
  OS-unfocused window, and with "no permission prompt visible"). Not a
  transport defect. Fix: a distinct `TransportError::Timeout` variant plus
  clipboard classification that names the unsettled-promise cause; browser
  command rejections (`Protocol`) classify as stale document/world.
- **#9 root cause (recorded, fixed here)**: the lazy download authority
  subscribed `Browser.downloadWillBegin`/`downloadProgress` and enabled
  `Browser.setDownloadBehavior` only at first `list_downloads`
  (`session/downloads.rs` ~48-97). A download triggered before that first
  call was structurally unobservable (events not enabled, download path not
  managed), and the `Option` inventory cursor left no wait anchor. Both are
  design defects fully explaining the macOS observation; eager activation +
  never-absent cursor remove them. Deterministic doubles + fault injection
  carry verification since the original run cannot be reproduced locally.
- **Download delta keys on a retained `begun_sequence`**, not the mutable
  `public.sequence`: `sequence` bumps on every transition, so an unrelated
  pre-action download's progress would otherwise be attributed to this
  interaction. The internal `Entry` retains the sequence assigned at
  `begin`; the postcondition delta reports downloads whose begin happened
  after the pre-action cursor, with their current state.
- **`read_clipboard` stays recordless**: it is registry-declared read-only
  (not state-changing), so no evidence sink write exists for it; its outcome
  reaches the caller directly and its failure classification improves at the
  boundary. Only `WriteClipboard` records are enriched (a confirmed fact —
  the in-page bridge returned `true`).
- **Record shape changes incompatibly again → store schema v9 → v10 in the
  same stride** (`crates/krometrail-store/src/index/schema.rs`
  `CURRENT_SCHEMA_VERSION`), clearing only the known recording-cache members
  per Current Contract Discipline. `InteractionPostcondition` gains `Vec`
  fields and loses `Copy`; construction sites adapt compile-driven.
- **Bounded collections**: `MAX_SIDE_CHANNEL_FACTS = 4` per list (new pages,
  downloads), enforced at construction with exact `omitted` counts; wire
  decoding rejects over-cap lists. Attempt counts are `Option<u32>` where
  `None` means the signal source is unavailable (module convention: absence
  is "not observed", never a claimed failure); counts are documented as
  lower bounds under broadcast lag.
- **Cursors ride in the record**: `new_pages.cursor_before` and
  `downloads.cursor_before` are included so a caller can chain
  `wait_for_page(after: …)` / `wait_for_download(after: …)` directly from
  the interaction result — sequences are opaque ordinals, privacy-safe.

## Architectural choice

Session-layer post-dispatch pull-reconciliation + control-layer passive
signals (chosen) over two alternatives:

- **(B) Assemble popup facts from queued supervisor events after the
  operation** — rejected: `Execute` is handled serially in
  `run_supervisor` (session/runtime.rs ~637, ~755); queued `TargetCreated`
  inputs are unreachable until the operation returns, and draining the
  command channel mid-operation would break the single-writer reducer
  discipline.
- **(C) A dedicated side-channel observation service/port** — rejected:
  machinery with one caller; the facts derive from existing authorities
  (target reducer state, download authority, page-signal broadcast) exactly
  as the simplification arc demands.

The split of responsibilities: the control layer
(`execute_interaction_request_inner`) collects only session-scoped attempt
signals (window-open attempts, download requests, committed main-frame
navigation) — these are CDP session events independent of target reduction.
The session layer (`execute_operation`) collects the two inventory deltas
(pages, downloads) after target reconciliation and attaches them to the
already-built record before evidence persistence.

## Implementation Units

### Unit 1: Extended postcondition domain types
**File**: `crates/krometrail-core/src/browser/postcondition.rs` (+ re-exports in `browser/mod.rs`, `lib.rs`)
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts`

```rust
/// Canonical cap for per-interaction side-channel fact lists.
pub const MAX_SIDE_CHANNEL_FACTS: usize = 4;

/// One page adopted by the target supervisor after the pre-action cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NewPageFact {
    pub target_id: TargetId,
    pub sequence: PageSequence,
    /// The new page's opener is the acting interaction target.
    pub opener_matched: bool,
}

/// New-page inventory delta. Absent (None on the parent) when post-dispatch
/// reconciliation was unavailable — never a claim that nothing opened.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NewPagePostcondition {
    pub cursor_before: PageSequence,
    pub pages: Vec<NewPageFact>,   // len <= MAX_SIDE_CHANNEL_FACTS
    pub omitted: u32,              // exact count beyond the cap
}

impl NewPagePostcondition {
    /// Caps the list and records the exact omission count.
    pub fn from_observed(cursor_before: PageSequence, pages: Vec<NewPageFact>) -> Self;
}

/// One download whose begin was recorded after the pre-action cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DownloadFact {
    pub download_id: DownloadId,
    pub sequence: DownloadSequence,
    /// State at the observation point, not a terminal claim.
    pub state: DownloadState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DownloadPostcondition {
    pub cursor_before: DownloadSequence,
    pub downloads: Vec<DownloadFact>, // len <= MAX_SIDE_CHANNEL_FACTS
    pub omitted: u32,
}

impl DownloadPostcondition {
    pub fn from_observed(cursor_before: DownloadSequence, downloads: Vec<DownloadFact>) -> Self;
}

/// Session-scoped attempt signals drained at the observation point.
/// `None` = signal source unavailable; counts are lower bounds under lag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SideChannelSignals {
    /// Page.windowOpen occurrences (open attempts; no blocked/succeeded claim).
    pub window_open_attempts: Option<u32>,
    /// Page.frameRequestedNavigation with disposition "download"
    /// (download-attempt candidates).
    pub download_requests: Option<u32>,
}

pub struct PagePostcondition {
    pub url_changed: Option<bool>,
    pub navigation_lifecycle_observed: bool,
    /// A committed main-frame navigation (Page.frameNavigated) or main-frame
    /// same-document navigation (Page.navigatedWithinDocument) arrived
    /// between dispatch and observation. None = signal unavailable.
    /// Catches same-URL reloads and committed-and-returned navigations that
    /// `url_changed` misses; `url_changed` stays a separate fact.
    pub main_frame_navigation_observed: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionPostcondition {
    pub page: PagePostcondition,
    pub target: TargetPostcondition,
    pub signals: SideChannelSignals,
    pub new_pages: Option<NewPagePostcondition>,
    pub downloads: Option<DownloadPostcondition>,
    /// Some(true) only on WriteClipboard records whose in-page bridge
    /// confirmed the write. None everywhere else.
    pub clipboard_write_confirmed: Option<bool>,
}

impl InteractionPostcondition {
    // Existing signature gains the main-frame navigation fact:
    pub fn from_facts(
        pre: Option<&NodeStateFacts>,
        post: Option<&NodeStateFacts>,
        url_changed: Option<bool>,
        navigation_lifecycle_observed: bool,
        main_frame_navigation_observed: Option<bool>,
        signals: SideChannelSignals,
    ) -> Self;                          // new_pages/downloads/clipboard start None
    pub const fn unobserved() -> Self;  // all None/unobserved, as today
    /// WriteClipboard record block: unobserved() + clipboard_write_confirmed.
    pub const fn clipboard_confirmed() -> Self;
    /// Session-layer attachment after target reduction.
    pub fn attach_new_pages(&mut self, facts: NewPagePostcondition);
    pub fn attach_downloads(&mut self, facts: DownloadPostcondition);
}
```

**Implementation Notes**:
- `NewPagePostcondition`/`DownloadPostcondition` decode through
  `deserialize_validated` wires rejecting `pages.len() > MAX_SIDE_CHANNEL_FACTS`
  (validated-wire-contracts; `omitted` itself is not re-derivable from wire
  and passes through).
- `DownloadState`/`DownloadSequence`/`DownloadId`/`PageSequence` need
  `Deserialize` derives where missing (record round-trips through
  `record_json`).
- `InteractionPostcondition` loses `Copy`; `TargetPostcondition`,
  `FlagObservation`, `NodeStateFacts` keep theirs.

**Acceptance Criteria**:
- [ ] `from_observed` truth table: over-cap input → capped list + exact
      `omitted`; wire decode rejects an over-cap list.
- [ ] Round-trip through serde of a fully-populated block and of
      `unobserved()` / `clipboard_confirmed()`.

---

### Unit 2: Signal plumbing (window-open, download-request, committed navigation)
**Files**: `crates/krometrail-cdp/src/events/signals.rs`, `crates/krometrail-cdp/src/events/domain.rs`
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts`

```rust
// signals.rs
pub(crate) enum PageSignalKind {
    Lifecycle,
    DialogOpening,
    WindowOpen,           // Page.windowOpen
    DownloadRequested,    // Page.frameRequestedNavigation, disposition == "download"
    NavigationCommitted,  // main-frame Page.frameNavigated | Page.navigatedWithinDocument
}

impl PageSignalReceiver {
    /// Drains delivered signals and counts matches; lag skips are ignored
    /// (count is a lower bound), closure ends the drain.
    pub(crate) fn observed_count(&mut self) -> u32;
}
```

**Implementation Notes** (domain.rs):
- Promote `"Page.frameNavigated"` and `"Page.navigatedWithinDocument"` into
  the `operation_signal` match (~221-229) so they install even when the
  semantic pipeline is off; persistence remains gated by
  `runtime.persist_events` in the pump (unchanged).
- New `install_signal_only_source(runtime, transport, method)` for
  `"Page.windowOpen"` and `"Page.frameRequestedNavigation"`: mirrors
  `install_non_network_source` but never normalizes/persists and install
  failure degrades silently — no `unavailable` class accounting, because
  these sources feed postcondition facts only (facts report `None`).
- Pump signal mapping (~758-765): `Page.windowOpen` → `WindowOpen`;
  `Page.frameRequestedNavigation` → `DownloadRequested` only when
  `params.disposition == "download"`; `Page.frameNavigated` → record the
  main frame id (`frame.parentId` absent) in a new
  `TargetEventRuntime.main_frame: Mutex<Option<String>>` and send
  `NavigationCommitted` only for the main frame;
  `Page.navigatedWithinDocument` → `NavigationCommitted` when `frameId`
  equals the recorded main frame.
- `page_signal()` method table (~372-375) gains the three kinds; the
  installed-set check gives the control layer its availability answer
  (`Err(Unsupported)` → fact `None`). `NavigationCommitted` maps to
  `"Page.frameNavigated"` for the installed check.

**Acceptance Criteria**:
- [ ] Pump tests: child-frame `frameNavigated` does not signal; main-frame
      does; `navigatedWithinDocument` follows the recorded main frame;
      `frameRequestedNavigation` with disposition `newTab` does not signal
      `DownloadRequested`, `download` does; `windowOpen` signals.
- [ ] `observed_count` counts multiple deliveries and drains exactly once.
- [ ] Signal-only install failure leaves event classes available.

---

### Unit 3: Interaction-path signal facts
**File**: `crates/krometrail-cdp/src/control/interaction.rs`
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts`

In `execute_interaction_request_inner`, alongside the existing passive
`lifecycle_observation` subscription (~190-192): subscribe passive receivers
for `WindowOpen`, `DownloadRequested`, and `NavigationCommitted` before
dispatch (each `browser_events.page_signal(&event_binding, kind).ok()`); at
the assembly point (~341-353) drain them:

```rust
let signals = SideChannelSignals {
    window_open_attempts: window_open.as_mut().map(PageSignalReceiver::observed_count),
    download_requests: download_requested.as_mut().map(PageSignalReceiver::observed_count),
};
let main_frame_navigation_observed = navigation_committed
    .as_mut()
    .map(|receiver| receiver.observed_count() > 0);
let postcondition = InteractionPostcondition::from_facts(
    /* …existing args… */, main_frame_navigation_observed, signals,
);
```

**Implementation Notes**:
- Setup failure never fails dispatch (mirrors the lifecycle subscription);
  never adds wait time — drains only.
- HandleDialog takes the same passive subscriptions (they cannot block).

**Acceptance Criteria**:
- [ ] Scripted double: interaction with a windowOpen event delivered before
      the observation point reports `window_open_attempts: Some(1)`.
- [ ] Unsupported signal source degrades the facts to `None` while the
      interaction still succeeds.
- [ ] Same-URL committed navigation double: `url_changed: Some(false)` with
      `main_frame_navigation_observed: Some(true)`.

---

### Unit 4: Post-dispatch reconciliation and delta attachment
**Files**: `crates/krometrail-cdp/src/session/operations.rs` (seam), `crates/krometrail-cdp/src/session/evidence.rs` (WriteClipboard block)
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-popup-navigation-facts` (pages), `…-download-authority-facts` (downloads delta)

```rust
// operations.rs
/// One pull-based target reconciliation: Target.getTargets → parse →
/// reduce(InitialTargets) → apply_effects → publish shared state. Extracted
/// from the wait_for_page loop body and reused there verbatim.
pub(super) async fn reconcile_targets_once(
    state: &mut SupervisorState,
    transport: &Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
) -> Result<()>;

const SIDE_CHANNEL_RECONCILE_WINDOW: Duration = Duration::from_secs(2);
```

In `execute_operation`, gated on `kind.is_interaction()`:
1. Before `execute_operation_unfenced`: capture
   `page_cursor_before = state.page_contexts()?.cursor` (pre-action by the
   serialization guarantee) and
   `download_cursor_before = shared.downloads.as_ref().and_then(|d| d.cursor())`.
2. After a successful result, before `persist_result_evidence`: run
   `tokio::time::timeout(SIDE_CHANNEL_RECONCILE_WINDOW, reconcile_targets_once(…))`.
   - On success: `NewPagePostcondition::from_observed(page_cursor_before,
     pages)` where pages = `state.page_contexts()` entries with
     `sequence > page_cursor_before`, `opener_matched =
     page.opener_target_id == Some(acting_target)` (acting target from the
     result record's context). Attach via a
     `fn enrich_interaction_record(&mut BrowserOperationResult, …)` that
     matches the nine interaction variants and mutates
     `value.record.postcondition`.
   - On timeout/failure: leave `new_pages: None` (reconciliation
     unavailable); the interaction result is never failed or delayed.
3. Downloads (story 2): when the authority is active, delta = downloads with
   `begun_sequence > download_cursor_before` →
   `DownloadPostcondition::from_observed(download_cursor_before, facts)`;
   authority inactive/unavailable or attach-mode ownership → `downloads: None`.
4. `evidence.rs`: the WriteClipboard projection uses
   `InteractionPostcondition::clipboard_confirmed()` instead of
   `unobserved()`.

**Implementation Notes**:
- Enrichment runs before persistence, so the retained record and the live
  response carry identical facts (one authority, projected twice).
- The full-inventory reduce may destroy targets that vanished mid-action
  (e.g. the click closed its own page) — that is the honest post-state and
  the machinery is already proven by `wait_for_page`; the queued
  `TargetCreated` that arrives after `Execute` returns is idempotent against
  the already-reconciled key (reducer `reconcile_one` existing-key path).
- Cost: one `Target.getTargets` round-trip per state-changing interaction
  (including per batch step). Bounded, silent, no retries; a measured-cost
  concern routes to a `[perf]` item later.

**Acceptance Criteria**:
- [ ] Scripted double: getTargets grows by one page with `openerId` = acting
      target → `new_pages.pages == [{opener_matched: true, …}]`,
      `cursor_before` = pre-action cursor; record persisted with the block.
- [ ] No growth → empty `pages`, `omitted: 0`, cursor present (honest
      "no new page observed").
- [ ] Reconciliation transport failure/timeout → `new_pages: None`, action
      still succeeds and persists.
- [ ] Acting target missing from the post inventory → no panic, facts
      assembled from the remaining state.
- [ ] Batch: each step's record carries its own side-channel block.
- [ ] WriteClipboard record: `clipboard_write_confirmed: Some(true)`.

---

### Unit 5: Eager download activation and the never-absent cursor
**Files**: `crates/krometrail-cdp/src/session/downloads.rs`, `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-core/src/browser/local_io.rs`
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-download-authority-facts`

```rust
// downloads.rs — LazyManagedDownloadAuthority becomes:
pub(crate) struct ManagedDownloadControl {
    session_id: SessionId,
    base_root: PathBuf,
    ids: Arc<dyn IdSource>,
    subscribers: Arc<SubscriberRegistry>,
    active: std::sync::OnceLock<Arc<ManagedDownloadAuthority>>,
    unavailable: Mutex<Option<KrometrailError>>,
}

impl ManagedDownloadControl {
    /// Called once at managed session start, before any interaction can
    /// dispatch. Failure stores the unavailable error; the session starts.
    pub(crate) async fn activate(&self, transport: Arc<dyn CdpTransport>) -> Result<()>;
    /// Sync pre-dispatch cursor capture; None only when not activated.
    pub(crate) fn cursor(&self) -> Option<DownloadSequence>;
    /// Sync begun-after delta for postcondition assembly.
    pub(crate) fn begun_after(&self, cursor: DownloadSequence) -> Vec<DownloadFact>;
    // list/wait_with_cancellation/cancel/read/rebind/shutdown keep their
    // shapes; list no longer activates.
}

// State seeding mirrors pages: sequence 1 is the empty-inventory cursor.
// next_sequence: 2; Entry gains `begun_sequence: DownloadSequence` retained
// at begin.

// core local_io.rs — one cursor contract:
pub struct DownloadInventory {
    pub session_id: SessionId,
    pub cursor: DownloadSequence,      // was Option — never absent
    pub downloads: Vec<ManagedDownload>,
}
pub struct WaitForDownloadRequest {
    pub after: DownloadSequence,       // was Option — now required
    pub download_id: Option<DownloadId>,
    #[serde(default)] pub terminal: bool,
    #[serde(default = …)] pub timeout: u64,
}
```

**Implementation Notes**:
- `session/mod.rs` (~421-430): construct, then `activate(transport)`
  best-effort before `SessionShared` is published — guarantees
  subscribed/enabled before any interaction dispatch. Reconnect keeps
  `rebind` unchanged.
- Delete the `activated()` error ("call list_downloads before triggering a
  download") and the Option-cursor `after.map_or(0, …)` special case.
- Wire change is breaking by design (Current Contract Discipline — no
  compat alias): regenerate canonical JSON schema artifacts
  (`bash scripts/check-wire-enum-schemas.sh` must pass), update the
  `wait_for_download` tool description to name the two cursor sources
  (list_downloads cursor, interaction postcondition `downloads.cursor_before`).
- Roll `docs/SPEC.md` Local Data forward in the same stride: downloads carry
  a never-absent cursor seeded like the page cursor; `wait_for_download`
  requires `after`.

**Acceptance Criteria**:
- [ ] Managed session start activates: subscribe-before-enable order
      preserved; activation failure stores unavailable, session starts, and
      explicit download ops report the stored error.
- [ ] Empty inventory reports `cursor == 1` (never absent).
- [ ] Interaction + injected `downloadWillBegin` → postcondition
      `downloads.downloads == [{state: in_progress, …}]` and
      `wait_for_download(after: cursor_before)` observes it.
- [ ] A pre-action download progressing during the interaction does NOT
      appear in the delta (begun_sequence discipline).
- [ ] Attach-mode session: interaction carries `downloads: None`.

---

### Unit 6: Clipboard failure classification
**Files**: `crates/krometrail-cdp/src/transport/error.rs`, `crates/krometrail-cdp/src/transport/cdpkit.rs`, `crates/krometrail-cdp/src/control/clipboard.rs`
**Story**: `epic-state-aware-interaction-results-side-channel-outcomes-clipboard-classification`

```rust
// transport/error.rs
pub enum TransportError {
    InvalidInput, ConnectFailed, CommandFailed, Protocol,
    Timeout,          // new: command sent, no response within the window
    Disconnected, SubscriptionClosed, Closed,
}
// is_retryable: Timeout is NOT transport-retryable (same as CommandFailed).

// cdpkit.rs map_error: CdpError::Timeout => TransportError::Timeout.
```

`clipboard_dispatch_error` classifies what is knowable:
- `Timeout` → `InteractionFailed`, message "clipboard operation did not
  settle before the command deadline — the browser may be holding an
  unresolved clipboard permission decision or the window is not focused at
  the OS level" (class `command_timeout`); recovery: focus the managed
  browser window at the OS level, resolve any permission prompt, retry.
- `Protocol` (browser rejected the command — destroyed context/world) →
  `StaleReference`, "clipboard document or isolated world was destroyed
  while the operation was in flight"; recovery: re-inspect and retry.
- `CommandFailed`/others → current generic transport-class message;
  `Disconnected` unchanged.

**Implementation Notes**:
- All exhaustive `TransportError` matches update compile-driven
  (`transport_error_class`, control/mod.rs mapping, doubles).
- The in-page typed errors (secure_context/focus/clipboard_unavailable/
  permission-denied) already classify correctly via `exceptionDetails` and
  are untouched.
- Record the #8 conclusion in tool-facing prose only to the extent the
  message above does; no speculative "blocked" claims.

**Acceptance Criteria**:
- [ ] `clipboard_dispatch_error(Timeout)` names the unsettled operation and
      pending-permission possibility; no longer claims a transport error.
- [ ] `clipboard_dispatch_error(Protocol)` → `StaleReference`.
- [ ] `Disconnected` still propagates `BrowserDisconnected`.
- [ ] cdpkit timeout maps to `Timeout`; existing transports compile with the
      new variant.

---

### Unit 7: Store schema bump, projection, and qualification
**Files**: `crates/krometrail-store/src/index/schema.rs`, `crates/krometrail-store/src/index/interactions.rs` (tests), `crates/krometrail-mcp/src/response.rs` (tests), `crates/krometrail-cdp/tests/` qualification
**Story**: split — schema bump + response tests in `…-popup-navigation-facts`; real-Chrome qualification in `…-download-authority-facts`

- `CURRENT_SCHEMA_VERSION: u32 = 9` → `10` (incompatible `record_json`
  postcondition shape); existing clear-on-mismatch machinery covers the
  rest — update version assertions.
- MCP projection is automatic (the concise block IS the record field);
  update response-shape tests for the new fields.
- One new gated real-Chrome qualification (`KROMETRAIL_REAL_CHROME_TESTS`),
  warranted because no real-Chrome download coverage exists today (advisory
  note) and the #9 arc is otherwise only double-verified: fixture page with
  an `<a download>` link and a `window.open` button —
  (a) click the download link → postcondition `downloads` fact present and
  `wait_for_download(after: cursor_before, terminal: true)` completes;
  (b) click the open button → `new_pages` fact with `opener_matched: true`
  and `window_open_attempts >= 1`.

**Acceptance Criteria**:
- [ ] Store round-trip decodes a fully-populated side-channel block at v10;
      v9 cache clears on open.
- [ ] Concise interaction response carries the extended block.
- [ ] Gated qualification passes against local Chrome.

---

## Implementation Order
1. `side-channel-outcomes-popup-navigation-facts` (Units 1, 2, 3, 4-pages,
   7-schema/projection)
2. `side-channel-outcomes-download-authority-facts` (Units 5, 4-downloads,
   7-qualification) — depends on 1
3. `side-channel-outcomes-clipboard-classification` (Unit 6 + the
   `clipboard_confirmed` record wiring from Unit 4.4) — depends on 1 (record
   field), independent of 2

## Simplification
- One "cursor is never absent" contract across pages and downloads: the
  `Option` download cursor, `after.map_or(0, …)`, and the `activated()`
  "call list_downloads first" recovery trap are deleted (Unit 5).
- `LazyManagedDownloadAuthority` collapses to an eagerly-activated
  `ManagedDownloadControl`; the lazy activation special case and its
  interplay prose are removed.
- No new event stream: attempt signals reuse the existing per-target
  page-signal broadcast; popup facts reuse the existing reducer +
  `wait_for_page`'s pull pattern (now a shared `reconcile_targets_once`).
- Tests asserting Option-cursor/lazy behavior are retired with the behavior
  (`managed_and_named_profile_defaults_stay_inert_until_first_list` and
  friends are rewritten for the eager contract).

## Testing
- Core truth tables: cap/omission constructors, over-cap wire rejection,
  serde round-trips (Unit 1) — protects the bounded-collections and
  validated-wire contracts.
- Pump/signal tests: main-frame filtering, disposition filtering,
  `observed_count` drain semantics, signal-only install degradation
  (Unit 2) — protects the honest attempt/no-outcome semantics.
- CDP deterministic doubles (layered-cdp-qualification tier 1): popup delta
  with opener match, empty delta honesty, reconciliation fault injection →
  `None`, batch-step inheritance, download begin delta, begun-sequence
  discipline, attach-mode `None`, activation-failure degradation
  (Units 3-5).
- Transport/classification unit tests: Timeout vs Protocol vs CommandFailed
  clipboard classification (Unit 6) — protects the #8 fix.
- Store v10 round-trip + clear-on-mismatch; MCP response-shape (Unit 7).
- One gated real-Chrome qualification for download + popup end-to-end
  (Unit 7) — the only tier-3 addition; everything else stays deterministic.

## Risks
- **Popup adoption latency**: a `window.open` target may not appear in
  `Target.getTargets` by the assembly point. The block then honestly shows
  `window_open_attempts >= 1` with an empty `pages` list and a
  `cursor_before` anchor for `wait_for_page` — attempt/no-outcome, never
  "blocked". Documented in the field docs; expectation-notes must not treat
  empty `pages` + attempts as a contradiction.
- **Full-inventory reduce side effects mid-execute** (destroy/attach): the
  pattern is already exercised by `wait_for_page`; the acting-target-closed
  double covers the sharpest case. Fallback if an unforeseen interaction
  with capture/attach effects surfaces: scope the reconciliation to a
  targeted `Target.getTargetInfo` per new key (weaker but effect-free).
- **`Page.windowOpen`/`frameRequestedNavigation` availability drift** across
  Chromium versions: facts degrade to `None` via the installed-set check;
  the gated qualification pins current behavior.
- **`TransportError::Timeout` blast radius**: exhaustive matches across
  transport doubles and error mappers; compile-driven, and `is_retryable`
  keeps Timeout non-retryable so reconnect behavior is unchanged.
- **Per-interaction cost** (+1 `Target.getTargets`, +3 passive
  subscriptions): bounded and silent; measured regressions route to a
  `[perf]` item rather than back into this design.
- **Second store-format bump inside one epic** (v9 → v10): acceptable under
  Current Contract Discipline (agent-tool contract, cache-only clear);
  configuration/profiles/diagnostics survive per the schema module's
  existing discipline.

## Implementation notes

All three child stories landed in design order and are at `done`; per-story
detail lives in their `## Implementation` sections. Commits:
`22c14364` (popup/navigation facts + schema v10 + review fixes),
`adfc397c` (eager download authority + never-absent cursor + real-Chrome
qualification), `61ceb77d` (Timeout classification + clipboard record fact).

Feature-level summary against the design:

- **All seven implementation units landed as designed.** The one structural
  refinement: the session-layer enrichment is two helpers
  (`attach_new_page_facts` — the bounded reconciliation pull — and
  `attach_download_facts` — a lock-read with no browser round-trip) sharing
  `interaction_record_mut` over the nine interaction variants, rather than a
  single `enrich_interaction_record`; same seam, same ordering (after
  `execute_operation_unfenced`, before `persist_result_evidence`), batch
  steps inherit through recursion as designed.
- **Binding advisory constraints honored**: pre-action cursors are read
  under the supervisor's serial `Execute` guarantee and post-state comes
  only from the pull reconciliation (`reconcile_targets_once`, extracted
  from and reused by `wait_for_page`); the interaction path never reads the
  page cursor as post-state. Attempt signals ride the existing page-signal
  broadcast with `Page.windowOpen`/`Page.frameRequestedNavigation`
  signal-only (silent degradation, no event-class accounting, no
  BrowserEventKind growth) and
  `Page.frameNavigated`/`navigatedWithinDocument` promoted to
  always-installed operation signals with main-frame filtering. Facts are
  attempt/outcome/no-outcome only — no "blocked" claims anywhere. Bounded
  lists carry `MAX_SIDE_CHANNEL_FACTS = 4` caps with exact omission counts
  enforced at construction and over-cap wire rejection. Cursors ride in the
  record (`cursor_before` on both deltas) for direct
  `wait_for_page`/`wait_for_download` chaining. Clipboard scope stayed
  narrowed: no automatic probing; only the explicit-write record gained the
  bridge-confirmed fact, and `read_clipboard` stays recordless. The F5
  navigation-fact upgrade landed as `main_frame_navigation_observed`
  alongside the retained `url_changed` and `navigation_lifecycle_observed`
  facts, with signal availability recorded through `Option`.
- **Root causes**: #9 (lazy activation + Option cursor) fixed by eager
  activation at session start and the one never-absent cursor contract, and
  qualified end-to-end against real Chrome. #8 (Timeout→CommandFailed
  collapse) confirmed by fault injection at the cdpkit mapper and fixed with
  the distinct `Timeout` category plus honest classification.
- **Cross-model review fixes** for the parked postcondition-core findings
  were folded into story 1 (typed `Unobserved` target outcome with the
  detachment claim now requiring an observed `connected: false`, fenced
  signal attribution via pump-stamped observation times, 250ms pre-URL
  budget with the post-probe made concurrent, and the version-agnostic
  future-schema test seeded at 9999).
- **Simplification delivered**: `LazyManagedDownloadAuthority` and its
  activation trap deleted; one cursor contract across pages and downloads;
  no new event stream; the `wait_for_page` pull body now shared; lazy-path
  tests retired with the behavior.
- **Contract changes for the review pass to note**: store schema v9 → v10
  (cache-only clear); `WaitForDownloadRequest.after` required and
  `DownloadInventory.cursor` non-optional (breaking, no compat alias, per
  Current Contract Discipline); `wait_for_download` tool description and
  SPEC.md rolled forward; `InteractionPostcondition` lost `Copy`.
- **Known accepted risks** (from the design, unchanged): popup adoption
  latency yields attempts >= 1 with an empty delta plus the cursor anchor
  (the real-Chrome qualification tolerates this honestly); the
  per-interaction cost (+1 `Target.getTargets`, +3 passive subscriptions,
  and inside batch steps the reconcile shares the step deadline) routes to a
  `[perf]` item if measured.

## Review adjudication (standard weight, cross-model gpt-5.6-sol, one pass)

Nine areas verified clean (enrichment placement, Unobserved handling, pre-URL
bound, eager activation ordering, cursor contract, bounded collections,
clipboard privacy, schema v10 exact-version replacement, gated qualification).
Five findings, all accepted; fixes routed to the implementation worker,
closure is fix-verification only:

1. (blocker) Reconciliation timeout wraps a state-mutating phase — a timeout
   during `Target.attachToTarget` can drop the future after authoritative
   state replacement, stranding a discovered target. Bounded phase must be
   read-only or cancellation-safe; stalled-attach regression test required.
2. (blocker) The new `TransportError::Timeout` bypasses the
   ambiguous-but-dispatched pointer policy (`pointer.rs` treats only
   `CommandFailed` as dispatched) — a real timeout now hard-fails popup
   clicks. Include `Timeout` in that branch; test it directly.
3. (significant) Page/download baselines captured at `execute_operation`
   entry, before preflight — popups/downloads begun during preflight are
   misattributed. Capture baselines immediately before input dispatch.
4. (significant) Signal fencing stamps pump-dequeue time, not event
   occurrence — pump backlog can leak a prior interaction's event past the
   floor. Fix at transport ingress; park with rationale if invasive.
5. (significant) `tokio::join!` waits out a stalled 2s probe after
   observation completes — probe becomes subordinate to observation
   completion (optional evidence never extends the result).

## Review fixes

- **A1 fixed:** target reconciliation now separates the cancellable,
  read-only `Target.getTargets` fetch from the uncancelled reduction/state
  publication and attach-effects phase. A paused stalled-attach regression
  proves the supervisor completes attachment after the side-channel deadline.
- **A2 fixed:** pointer gesture `TransportError::Timeout` now follows the
  ambiguous-but-dispatched policy used for `CommandFailed`; direct timeout
  injection covers the popup-opening click case.
- **A3 fixed:** page and managed-download cursors are captured immediately
  before dispatch and returned through the interaction seam for enrichment.
  A preflight side-channel regression confirms an event delivered while target
  resolution is still running is excluded from the dispatched interaction.
- **A4 fixed:** production cdpkit `NamedEvent` values now carry a receipt
  `Instant` before pump queueing, and signal/persistence pumps project that
  ingress instant into session time. Test and alternate transports that omit
  receipt metadata explicitly retain the dequeue-clock fallback; those paths
  cannot make an ingress-order claim they did not record. A delayed-delivery
  ordering regression covers the two-interaction fence boundary.
- **A5 fixed:** live observation now wins the concurrent race and performs a
  single zero-grace poll of the optional probe; a paused stalled-probe test
  verifies the result returns after one second of settling time, before the
  two-second probe ceiling.
