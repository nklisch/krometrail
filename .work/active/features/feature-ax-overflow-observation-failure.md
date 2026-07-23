---
id: feature-ax-overflow-observation-failure
kind: feature
stage: implementing
tags: [browser-control]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# AX-overflow observation failure clarity

## Brief

On pages whose accessibility tree exceeds what Chrome will serialize (repro:
`https://html.spec.whatwg.org/` one-page spec, content height ~2.25M CSS px),
`snapshot_page` and `query_page` fail opaquely instead of degrading or guiding
recovery. Found during the v1.6.0 full-surface shakedown.

Observed on 1.6.0:

- Both tools return only the bare error string "browser rejected or could not
  complete the page observation command". No correlation id, no structured
  recovery surfaced in the MCP error result (contrast: degraded action
  responses on the same page carry structured warnings with recovery text).
- Log shows `mcp.response.failed`, `failure_stage: operation`,
  `error_code: page_observation_failed` and nothing else — no CDP-level event
  records which command failed or why, so "AX tree too large" is
  indistinguishable from any other observation failure.
- The recovery text that does exist elsewhere for this code ("retry once; if it
  fails again, inspect browser compatibility and status") is wrong for this
  cause: retries always fail (each attempt burns ~10 s) and compatibility is
  fine.
- Post-action live observation on such a page also degrades persistently:
  scroll/click responses report both `page_observation_failed` (snapshot) and
  `screenshot_failed`, while a standalone `take_screenshot` can still succeed.
  Control itself (scroll, coordinate/CSS click, fragment navigation) keeps
  working.

Fix direction: classify the oversized/unserializable-AX acquisition failure
distinctly (bounded detail from the CDP error), return the structured
limit-style error with honest recovery (viewport-anchored/frame-scoped
targeting, snapshot alternatives, "this page exceeds browser AX serialization"),
and log a bounded CDP failure event so diagnostics can attribute the cause.

## Simplification opportunity

The unified snapshot-limit recovery text introduced by
feature-query-node-limit-large-pages already carries frame-scoped-query
guidance; this failure class should reuse that recovery surface rather than
grow a parallel one.

## Grounding findings (code as it stands on 1.6.0)

Read `docs/SPEC.md` (observation-failure and recovery contract, lines ~186,
~254–255, ~608–616) and the code paths below. Key facts the design is built on:

- **Where the failure is produced.** `PageControl::capture_snapshot_for_frame`
  in `crates/krometrail-cdp/src/control/snapshot.rs` issues the two whole-page
  serialization commands: `Accessibility.getFullAXTree` (line ~452) and
  `DOMSnapshot.captureSnapshot` (line ~460). Both wrap a `send_raw` failure with
  `transport_error(error, ErrorCode::PageObservationFailed, target_id)`. That
  helper (`control/mod.rs::transport_error_for_surface`) yields the bare message
  "browser rejected or could not complete the page observation command" and
  `code.default_retry()` = `RetryAdvice::Safe` with the generic recovery
  "retry once; if it fails again, inspect browser compatibility and status"
  (`krometrail-core/src/error.rs` `default_recovery`). That is the exact wrong
  advice the brief names — retries re-run a serialization the browser already
  could not complete.

- **What the browser actually returns, and what survives the boundary.** cdpkit
  0.4.0 answers a rejected command with `CdpError::Protocol { code: i64, message:
  String }`; a command that outruns the 3 s command timeout
  (`src/app.rs` `with_command_timeout(Duration::from_secs(3))`) returns
  `CdpError::Timeout`. `transport/cdpkit.rs::map_error` deliberately collapses
  `CdpError::Protocol { .. }` → `TransportError::Protocol` and `CdpError::Timeout`
  → `TransportError::Timeout`, **discarding Chrome's code and message** (logged
  only at `tracing::debug!`). `TransportError` (`transport/error.rs`) is a
  bounded category enum by design ("cdpkit's source error is intentionally not
  stored in these values"). So by the time `snapshot.rs` sees the failure, the
  only bounded signal available is (a) *which* command failed and (b) the
  transport category. There is **no reliable Chrome-specific "AX tree too large"
  string** to key on — consistent with the brief's instruction to prefer honest
  stage-based classification over guessing.

- **How structured errors reach the agent.** The MCP layer never re-derives
  recovery; it carries the `KrometrailError` value through. Failed
  `snapshot_page` / `query_page` responses log `mcp.response.failed`
  (`krometrail-mcp/src/response.rs::fail_with`) and post-action live observation
  surfaces each `ObservationPart::Unavailable(error)` as a degraded warning
  (`degrade_with_stage`, event `mcp.response.degraded`). Both reuse the same
  `KrometrailError`, so fixing the classification at the CDP seam flows to the
  failed path *and* the degraded post-action path with no MCP change.

- **Correlation id.** `server.rs::call_tool` wraps the whole tool call in an
  `mcp.request` span carrying `correlation_id`. Neither `mcp.response.failed` nor
  `mcp.response.degraded` include the id as an event field — it is inherited from
  the enclosing span. A new CDP-boundary `tracing::warn!` emitted during the call
  is correlated the same way, with no explicit id threading.

- **The unified recovery text** lives inline in
  `snapshot.rs::snapshot_node_limit_error` (the `with_recovery(...)` string:
  "for queries, target a single frame with the `document` scope; for waits, poll
  a frame-scoped `query_page` instead, or capture the page with `snapshot_page`
  (which reports omitted nodes explicitly) and act on the returned node
  references directly"). This is the surface to reuse.

- **Precedent.** SPEC lines ~254–255 already describe a distinct dialog-open
  error "whose recovery is to handle the dialog, not to retry or inspect browser
  compatibility." This feature applies the identical shape to serialization
  failures.

## Architectural choice

The failure must be reclassified at the point a whole-page serialization command
fails, without weakening the deliberate `TransportError` privacy boundary or
touching the many call sites that pattern-match it.

**Option A — enrich the shared `TransportError` to carry the bounded CDP
protocol code + message, then classify from a generic seam.** Rejected. Adding
data to `TransportError::Protocol` breaks the exhaustive `match error` in
`control/clipboard.rs`, the `TransportError::CommandFailed | TransportError::Protocol`
patterns in `dialog.rs`/`pointer.rs`, and unit-variant construction in several
tests — broad churn for little gain. It also reverses the intentional decision
that "cdpkit's source error is not stored," and the numeric CDP code (typically a
generic `-32000` server error) adds essentially no classification power over the
transport category we already have.

**Option B — classify at the observation-serialization seam in `snapshot.rs`;
keep `TransportError` bounded; emit one bounded event there (CHOSEN).** Both
whole-page serialization commands are issued in one function. A small helper maps
any non-disconnect failure of those two commands to a classified
`page_observation_failed` error that carries an honest stage-named explanation,
the **shared** unified recovery text, and `RetryAdvice::Never`; and emits one
bounded `observation.serialization.failed` diagnostics event (stable name, error
code, failure stage, failed command, transport category), correlated by the
enclosing `mcp.request` span. The classified `KrometrailError` flows unchanged to
the failed `snapshot_page`/`query_page` response and to the degraded post-action
warning. The only shared-type change is a purely additive, data-free
`TransportError::category()` accessor for the event's bounded category field.

**Option C — emit the event inside the cdpkit transport adapter, where the raw
`CdpError` code/message still exists.** Rejected. The transport adapter is
semantically generic: it cannot know a given `send_raw` is "the observation
serialization step," so it would either fire this event for every command or need
semantic knowledge that does not belong at that layer. Surfacing Chrome's
free-form `message` there also pushes an unbounded string toward the log,
against the privacy discipline. Chrome's message stays where it already is —
`tracing::debug!` in the adapter — for deep local debugging only.

**Chosen: Option B.** It is the smallest change that satisfies all four goals,
reuses the existing recovery surface, respects the transport privacy boundary,
and needs zero MCP-layer edits because both failure surfaces already re-use the
same error value.

### Classification decision (goal 1, stated explicitly)

We cannot reliably distinguish "AX tree too large" from any other rejection of
the serialization command, because cdpkit discards Chrome's message and a 3 s
timeout and an answered rejection both reduce to bounded categories. The design
therefore classifies **by stage, honestly**: the anchor is "a whole-page
serialization command (`Accessibility.getFullAXTree` or
`DOMSnapshot.captureSnapshot`) failed," and the explanation says the page is
large enough that its accessibility/DOM tree commonly exceeds what the browser
returns in one observation — it does not assert a measured size we do not have.
The bounded detail carried is the transport **category** (`protocol`, `timeout`,
`command_failed`, …), which is genuinely derived from the CDP error response and
is safe (no page content, URL, or raw CDP traffic). Transport **disconnects**
(`Disconnected | Closed | SubscriptionClosed`) are not serialization failures and
keep the existing `browser_disconnected` boundary.

## Implementation Units

Single bundle, no child stories (one cohesive change across two crates, verified
by one deterministic test group).

**Unit 1 — bounded transport-category accessor.**
`crates/krometrail-cdp/src/transport/error.rs`. Add, additively (no variant data,
no behavior change):

```rust
impl TransportError {
    /// Stable, bounded category name for diagnostics. Carries no CDP code,
    /// message, or page content — only the fixed transport category.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::ConnectFailed => "connect_failed",
            Self::CommandFailed => "command_failed",
            Self::Protocol => "protocol",
            Self::Timeout => "timeout",
            Self::Disconnected => "disconnected",
            Self::SubscriptionClosed => "subscription_closed",
            Self::Closed => "closed",
        }
    }
}
```

(`control/clipboard.rs` already has an ad-hoc copy of this mapping; converging it
onto this accessor is an optional follow-up noted under Simplification, not
required by this feature.)

**Unit 2 — shared recovery constant + classified serialization error + bounded
event.** `crates/krometrail-cdp/src/control/snapshot.rs`.

- Extract the unified recovery string into a module const so both paths share one
  surface:

```rust
/// Unified recovery guidance shared by the node-limit and serialization-failure
/// observation paths: frame-scoped `document` targeting, `snapshot_page`
/// alternatives, and `take_screenshot` for pixels.
const SNAPSHOT_SCOPE_RECOVERY: &str = "for queries, target a single frame with the `document` scope; for waits, poll a frame-scoped `query_page` instead, or capture the page with `snapshot_page` (which reports omitted nodes explicitly) and act on the returned node references directly";

/// Stable diagnostics event emitted at the CDP boundary when a whole-page
/// serialization command fails, so the log can attribute the observation
/// failure without page content, URLs, or raw CDP traffic.
const OBSERVATION_SERIALIZATION_EVENT: &str = "observation.serialization.failed";
```

  `snapshot_node_limit_error` changes its `with_recovery(NonEmptyText::new(...))`
  literal to `NonEmptyText::new(SNAPSHOT_SCOPE_RECOVERY)` (behavior identical).

- Add the failed-command descriptor and classifier:

```rust
#[derive(Clone, Copy)]
enum SerializationCommand {
    AccessibilityTree,
    DomSnapshot,
}

impl SerializationCommand {
    const fn command(self) -> &'static str {
        match self {
            Self::AccessibilityTree => "Accessibility.getFullAXTree",
            Self::DomSnapshot => "DOMSnapshot.captureSnapshot",
        }
    }
    const fn stage(self) -> &'static str {
        match self {
            Self::AccessibilityTree => "accessibility_serialization",
            Self::DomSnapshot => "dom_serialization",
        }
    }
    const fn explanation(self) -> &'static str {
        match self {
            Self::AccessibilityTree => "the browser could not serialize this page's accessibility tree in one observation; a page this large commonly exceeds what the browser returns, so a full-page snapshot cannot complete",
            Self::DomSnapshot => "the browser could not serialize this page's DOM snapshot in one observation; a page this large commonly exceeds what the browser returns, so a full-page snapshot cannot complete",
        }
    }
}

/// Classify a failed whole-page serialization command. Transport disconnects
/// keep the existing disconnect boundary; every other failure becomes a
/// non-retryable page-observation error whose recovery routes to frame-scoped
/// queries and screenshots instead of a blind retry, and emits one bounded
/// diagnostics event correlated by the enclosing `mcp.request` span.
fn observation_serialization_error(
    error: TransportError,
    target_id: TargetId,
    command: SerializationCommand,
) -> KrometrailError {
    if matches!(
        error,
        TransportError::Disconnected
            | TransportError::Closed
            | TransportError::SubscriptionClosed
    ) {
        return transport_error(error, ErrorCode::PageObservationFailed, target_id);
    }
    tracing::warn!(
        event = OBSERVATION_SERIALIZATION_EVENT,
        error_code = ErrorCode::PageObservationFailed.as_str(),
        failure_stage = command.stage(),
        command = command.command(),
        transport_category = error.category(),
        "page serialization command failed"
    );
    KrometrailError::new(
        ErrorCode::PageObservationFailed,
        NonEmptyText::new(command.explanation())
            .expect("serialization failure explanation is non-empty"),
    )
    .with_context(ErrorContext {
        target_id: Some(target_id),
        ..ErrorContext::default()
    })
    .with_retry(RetryAdvice::Never)
    .with_recovery(
        NonEmptyText::new(SNAPSHOT_SCOPE_RECOVERY)
            .expect("snapshot scope recovery is non-empty"),
    )
}
```

  `TargetId` is already in scope; `TransportError` is already imported via
  `crate::transport::{... TransportError}`.

**Unit 3 — route the two serialization call sites through the classifier.**
`crates/krometrail-cdp/src/control/snapshot.rs`, inside
`capture_snapshot_for_frame`. Replace the two `map_err` closures:

- `Accessibility.getFullAXTree` (≈ line 455):
  `transport_error(error, ErrorCode::PageObservationFailed, bound.target_id)`
  → `observation_serialization_error(error, bound.target_id, SerializationCommand::AccessibilityTree)`
- `DOMSnapshot.captureSnapshot` (≈ line 473):
  → `observation_serialization_error(error, bound.target_id, SerializationCommand::DomSnapshot)`

  Scope note: only these two whole-page serialization **command** failures are
  reclassified. Malformed *successful* responses still route through
  `decode_ax_tree_with_ids` / `decode_dom_snapshot_with_geometry` → `malformed(...)`
  unchanged (the existing `{"nodes":"malformed"}` test path is unaffected). Other
  `page_observation_failed` sites (fingerprint, geometry, `query_selector`) keep
  the generic mapping — a transient failure there is legitimately retry-once.

**Unit 4 — post-action live observation (goal 4): no code change, verified by
test.** Post-action live observation runs the same `capture_snapshot_for_frame`;
its snapshot failure becomes `ObservationPart::Unavailable(classified_error)`,
which `response.rs` surfaces as a degraded warning. The classified explanation +
unified recovery + `retry: never` therefore flow to the degraded surface
automatically. A test asserts this rather than any new code.

### Give-up cache decision (goal 4, explicit)

**Do not add a persistent per-page "observation gave up" cache.** Rationale: the
only per-target state seam here, `SnapshotRegistry`, caches *successful* snapshot
generations keyed by document fingerprint; it has no natural slot for a sticky
failure verdict, and bolting one on would import fresh staleness/invalidation
logic (an SPA can shrink a page via client navigation, so a cached "too large"
verdict would go wrong silently) for marginal benefit. The real repeated-cost
avoidance is achieved more cheaply and honestly by `RetryAdvice::Never` plus the
frame-scoped/screenshot recovery: the agent is told, in the response itself, to
stop re-running the full-page snapshot and switch to a frame-scoped `query_page`
or `take_screenshot`, both of which avoid the whole-page serialization entirely.

## Implementation Order

1. Unit 1 (`TransportError::category`) — leaf, no dependents.
2. Unit 2 (const extraction + classifier + event) — depends on Unit 1.
3. Unit 3 (route both call sites) — depends on Unit 2.
4. Unit 4 (tests) — exercises Units 2–3 end to end.

## Simplification

- **Reuse, do not duplicate, the recovery surface.** One `SNAPSHOT_SCOPE_RECOVERY`
  const backs both the node-limit path and the new serialization-failure path;
  no parallel recovery string is introduced.
- **No `TransportError` churn.** The shared error type stays a bounded category
  enum; the only addition is a data-free `category()` accessor. This avoids the
  cross-cutting match/pattern breakage of Option A and preserves the deliberate
  "no stored cdpkit source error" privacy boundary.
- **No MCP-layer edits.** Because failed and degraded surfaces both re-use the
  same `KrometrailError`, the CDP-seam fix reaches both with nothing added in
  `krometrail-mcp`.
- **No failure cache** (see decision above).
- Optional follow-up (not in scope): converge `control/clipboard.rs`'s ad-hoc
  transport-category mapping onto `TransportError::category()`.

## Testing

Deterministic CDP-double tests only (no real browser), following
`layered-cdp-qualification` and modeled on the existing failure-injection tests
in `crates/krometrail-cdp/tests/page_observation.rs` (which already uses
`transport.push_failure("<method>", TransportError::…)` and asserts
`error.code`). Add to `page_observation.rs`:

1. **AX serialization-command failure classifies distinctly (snapshot_page).**
   Script the session to the snapshot point, `push_failure(
   "Accessibility.getFullAXTree", TransportError::CommandFailed)`, drive
   `SnapshotPageRequest`, `unwrap_err()`. Assert: `code == PageObservationFailed`;
   `retry == RetryAdvice::Never`; `recovery == Some(SNAPSHOT_SCOPE_RECOVERY)`;
   message mentions "serialize"/"accessibility"; and message is **not** the
   generic "browser rejected or could not complete…". This is the core defect
   guard.
2. **query_page path classifies identically.** Same injection, drive the
   role/name query entry that acquires the tree; assert the same fields.
3. **Category coverage.** Parameterize (or duplicate) case 1 for
   `TransportError::Timeout` and `TransportError::Protocol` — both must produce
   the same non-retryable classification (proves we key on the failed command +
   stage, not on a specific category).
4. **Disconnect regression guard.** `push_failure("Accessibility.getFullAXTree",
   TransportError::Disconnected)` → `code == BrowserDisconnected`,
   `retry == AfterRecovery` (i.e. the disconnect boundary is preserved, not
   swallowed into the serialization class).
5. **DOM serialization-command failure** (when DOM semantics/geometry are
   requested): `push_failure("DOMSnapshot.captureSnapshot",
   TransportError::CommandFailed)` after a successful AX tree → same
   non-retryable classification with the DOM-worded explanation.
6. **Post-action degraded flow carries the classification (goal 4).** Model on
   the existing `ObserveLive` test at `page_observation.rs:425`:
   `push_failure("Accessibility.getFullAXTree", …)`, drive
   `LiveObservationRequest`, `unwrap()`, and assert `live.snapshot` is
   `ObservationPart::Unavailable(error)` with `code == PageObservationFailed`,
   `retry == Never`, and the unified recovery — proving the same value reaches
   the degraded surface.
7. **Malformed-but-successful response is unchanged.** Keep/confirm the existing
   `{"nodes":"malformed"}` case still returns `PageObservationFailed` via the
   decode/`malformed` path (guards that Unit 3 narrowed to command failures only,
   not to all AX failures).

The bounded diagnostics event fields (`event`, `error_code`, `failure_stage`,
`command`, `transport_category`) are asserted structurally in code review against
Unit 2; the repo has no established tracing-event capture harness in these
integration tests, so introducing one is out of scope — the event is small,
literal, and covered by inspection alongside the mapping tests. If a
tracing-capture helper is added later, case 1 is the natural place to assert the
emitted fields.

## Risks

- **Over-broad classification.** Every non-disconnect failure of the two
  serialization commands now reads as a "page too large" class, including a rare
  genuinely-transient renderer hiccup on those commands. Mitigation: the
  explanation is hedged ("commonly exceeds…"), the recovery is valid for *any*
  serialization failure (frame-scoped query / screenshot both help regardless of
  cause), and `retry: never` steers to a cheaper next step rather than forbidding
  progress. Accepted as the honest trade the brief calls for.
- **`RetryAdvice::Never` is a behavior change on `page_observation_failed` for
  this path** (was `Safe`). This is a deliberate contract change (not a
  refactor): it is the whole point of the fix. SPEC's observation-failure/recovery
  language (≈ lines 186, 254–255, 608–616) already sanctions per-cause recovery
  and non-retry advice, so no SPEC edit is required; if any doc line drifts during
  implementation, keep it current per the documentation rules.
- **Event-name / field stability.** `observation.serialization.failed` and its
  fields join the stable diagnostics vocabulary; keep the name and the
  `transport_category` value set fixed (bounded strings via
  `TransportError::category()`), consistent with the other `*.failed` events.
- **Correlation depends on the enclosing span.** Outside an `mcp.request` span
  (e.g. a direct crate-level call in a test) the event has no `correlation_id`;
  this matches how `mcp.response.failed`/`degraded` already behave and is
  acceptable — correlation is a property of the MCP request boundary, not the CDP
  seam.
