---
id: epic-agent-browser-reliability-managed-session-lifecycle
kind: feature
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Reliable managed-browser lifecycle

## Brief

Correct GitHub issues #3, #4, and #5 across discovery, foreground interaction, and shutdown. Standard macOS Chrome discovery must tolerate a cold application version probe and report attempted candidates safely. Pointer operations against a hidden managed target must either recover through Krometrail-owned activation or return a specific actionable visibility failure rather than a generic observation error.

Shutdown results must describe remaining cleanup, not historical capture health: once the managed process/session/profile authority is released enough for an immediate restart, stop succeeds or returns an explicitly degraded result; a true incomplete result identifies the remaining resource safely.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: consumes durable diagnostic correlation; independent of capture outcome and input semantics implementation.

## Simplification opportunity
- Use the existing target activation and process authority instead of documenting external macOS automation as recovery.

## Foundation references
- `docs/SPEC.md` — managed lifecycle and stable error behavior
- `docs/ARCHITECTURE.md` — launcher, target supervisor, and shutdown ownership

## Design decisions

- **Cold discovery deadline**: keep version probing bounded but make its timeout an internal policy input and give platform-default candidates a cold-start-capable production budget. This corrects the standard macOS path without making `doctor` or startup unbounded, and a short injected budget keeps timeout regression tests fast.
- **Discovery diagnostics**: report candidate source, ordinal, probe outcome, and elapsed time to durable diagnostics, never executable/profile paths. Public `browser_not_found` recovery names the checked source classes and directs the caller to the correlated log; this follows the architecture privacy contract.
- **Hidden pointer targets**: pointer-like operations (`pointer`, `drag_drop`, and `scroll`) attempt both browser-target activation and page foregrounding before locator resolution when the supervisor says visibility is not `visible`. Activation does not change Krometrail's selected-target identity. If a bounded visibility recheck is still hidden or unknown, return the new stable `target_hidden` code with an explicit retry instruction.
- **Shutdown truth source**: classify stop from resources that remain owned after cleanup, not from historical capture state or an ancillary phase failure. Capture/event/detach/close-command failures become a degraded managed-close result once process/profile authority is safely released; only a positively remaining managed process/profile or attached transport authority returns `shutdown_incomplete`.
- **Stable stop compatibility**: retain `managed_browser_closed` and `detached` byte-for-byte and add `managed_browser_closed_degraded` only for paths that previously returned an error. This is corrective/additive for executable and MCP consumers; the unpublished Rust enum can gain the variant directly.
- **Foundation timing**: use code-first rolling-foundation updates. Implementation must replace the current `ARCHITECTURE.md` assertion that any bounded flush failure is incomplete with the resource-based distinction, and extend `SPEC.md` degraded-operation language without historical notes.
- **Dispatch rationale**: direct-read only. The launcher, pointer executor, and shutdown reducer are distinct but already have clear production seams and deterministic test doubles; exploratory fanout would not resolve a remaining design unknown.

## Architectural choice

### Considered approaches

1. **Increase the discovery timeout, document macOS focus recovery, and suppress shutdown errors.** This is the smallest patch, but it leaves probe behavior hard-coded, forces agents to use external automation, and can hide genuinely retained resources. It does not meet the actionable-failure or durable-diagnostics contracts.
2. **Introduce a new public lifecycle service and explicit focus/cleanup tools.** A lifecycle report object plus `activate_page` tool could model every intermediate state. It is expressive but unnecessarily expands the stable MCP surface and asks callers to orchestrate recovery that the existing process and target authorities can safely own.
3. **Strengthen the existing boundaries with private policy/report types and additive outcomes (chosen).** Discovery retains one policy and gains injectable probe timing; interaction preparation uses the existing target/session scopes; aggregate shutdown returns a private cleanup report that maps to existing or additive public outcomes. This minimizes public concepts while making each failure truthful and testable.

The chosen approach keeps domain-visible vocabulary in `krometrail-core`, CDP mechanics in `krometrail-cdp`, and MCP serialization derived from those values. The trickiest unit is aggregate shutdown because a failed phase is not equivalent to an owned resource remaining; it is designed first below.

## Implementation Units

### Unit 1: Resource-based aggregate shutdown report

**Files**:
- `crates/krometrail-cdp/src/session/shutdown.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`
- `crates/krometrail-cdp/src/session/reconnect.rs`
- `crates/krometrail-cdp/src/launcher/process.rs`
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/mod.rs`
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-cdp/src/session/mod.rs` (deterministic shutdown tests)

**Checkpoint**: `epic-agent-browser-reliability-managed-session-lifecycle-truthful-shutdown`

```rust
// crates/krometrail-cdp/src/session/shutdown.rs (private adapter vocabulary)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShutdownQuality {
    Clean,
    Degraded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RemainingResource {
    ManagedProcess,
    ManagedProfile,
    AttachedTransport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShutdownReport {
    pub(super) quality: ShutdownQuality,
    pub(super) remaining: Vec<RemainingResource>,
}

pub(super) async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    plan: ShutdownPlan,
) -> Result<ShutdownReport>;

// crates/krometrail-cdp/src/launcher/process.rs
pub(crate) fn force_kill_now(&mut self) -> bool;

// crates/krometrail-core/src/browser/session.rs
pub enum BrowserStopOutcome {
    ManagedBrowserClosed,
    ManagedBrowserClosedDegraded,
    Detached,
}
```

**Implementation notes**:

- Replace the single `failed` bit with independent ancillary degradation and remaining-resource accounting. Capture drain/flush, browser-event drain, target detach, and `Browser.close` command failure mark `Degraded`; they do not by themselves mean browser ownership remains.
- `capture::stop_target` must treat a stream already in `CaptureStreamState::Failed` as terminal capture history, not as evidence that stop failed. Worker join/deadline and accepted-frame abandonment still affect the shutdown report. Preserve capture gaps and diagnostics.
- `ManagedChromeProcess::force_kill_now` returns whether the direct child and any still-owned process-group members are gone. It remains safe to call from `Drop`; callers that can await retain the guard when verification fails rather than erasing the only evidence of an owned process.
- Release the managed `ProfileLease` only after verified process cleanup. Clear the transport only after pumps are aborted/detach attempted. Build `remaining` before returning; `shutdown_incomplete` must name the safe resource class (for example, “managed browser process remains after the shutdown deadline”) and use diagnostics for private cause detail, not expose a PID/path.
- Runtime/reconnect map `Clean` to the existing success variant and `Degraded` to `ManagedBrowserClosedDegraded` for managed sessions. Attached sessions return the existing `Detached` when local transport authority is released; an actually retained attached transport remains an error.
- Keep the absolute aggregate deadline. A deadline expiring after all resources are released is degraded success, not incomplete. Repeated `stop()` returns the cached identical result.
- Update `ErrorCode::ShutdownIncomplete` recovery to describe the named remaining authority and correlated diagnostics rather than generic process inspection.

**Acceptance criteria**:

- [x] A pre-existing failed capture stream with no abandoned accepted frames cannot turn an otherwise complete managed stop into `shutdown_incomplete`.
- [x] Capture/event flush or detach/`Browser.close` failure plus verified managed-process termination and profile release returns `managed_browser_closed_degraded` and permits immediate same-profile restart.
- [x] A clean stop still serializes exactly `managed_browser_closed`; attached cleanup still serializes exactly `detached`.
- [x] `shutdown_incomplete` is returned only while a concrete process, profile, or transport authority remains and its public message identifies that safe resource class.
- [x] The shutdown deadline remains one aggregate budget, and force cleanup cannot silently discard a process guard that still owns live members.

## Review record

- Effective weight: standard; pass: 1; verdict: approve after fixes.
- Findings fixed: `browser_not_found` recovery now names executable/environment/platform/PATH options and correlated diagnostics; shutdown truth is based on remaining owned authority rather than ancillary failures.
- Verification: cold-discovery budgets/deduplication, hidden-target activation, shutdown ownership, full workspace, and strict clippy tests passed.

### Unit 2: Bounded cold-start browser discovery

**Files**:
- `crates/krometrail-cdp/src/launcher/discovery.rs`
- `crates/krometrail-cdp/src/launcher/startup.rs`
- `src/app.rs`
- `tests/rust-runtime-smoke.rs`

**Checkpoint**: `epic-agent-browser-reliability-managed-session-lifecycle-cold-discovery`

```rust
// crates/krometrail-cdp/src/launcher/discovery.rs
#[derive(Clone, Copy, Debug)]
struct VersionProbePolicy {
    cold_candidate_timeout: Duration,
    ordinary_candidate_timeout: Duration,
    output_limit: u64,
}

fn discover_installations_with_policy(
    inputs: DiscoveryInputs,
    policy: VersionProbePolicy,
) -> Vec<BrowserInstallation>;

fn probe_version(
    path: &Path,
    timeout: Duration,
    output_limit: u64,
) -> VersionProbeOutcome;

#[derive(Debug)]
enum VersionProbeOutcome {
    Found(BrowserProduct, BrowserProductVersion),
    Missing,
    SpawnFailed,
    TimedOut,
    Rejected,
}
```

**Implementation notes**:

- Preserve public `discover_installations` and `discover_installations_with` signatures. They call the private policy function; tests inject millisecond-scale budgets.
- Give explicit/environment/platform-default candidates a production cold budget (target: 10 seconds) and PATH-only candidates the existing short ordinary budget (2 seconds). The standard macOS app path is already present and will be probed under the cold budget.
- Mark a canonical executable as seen before probing, so one slow/failing binary reachable through several sources/PATH entries is attempted once. Preserve the highest-precedence source of that first canonical occurrence.
- Preserve the 4096-byte stdout bound, null stdin/stderr, hard kill/wait, and no-filesystem-mutation behavior. The probe outcome contains no raw stdout or source error.
- Emit one sanitized diagnostic event per existing canonical candidate with `candidate_ordinal`, `candidate_source`, `probe_outcome`, and `elapsed_ms`; emit the completion event with attempted and discovered counts. Do not log executable paths.
- `LaunchError::BrowserNotFound` remains stable. Its safe recovery states which source classes were checked and points to diagnostics; `doctor` remains discovery-only and never launches a browser session.

**Acceptance criteria**:

- [x] A delayed version fixture that exceeds the ordinary budget but completes inside the cold budget is discovered when supplied as a platform default.
- [x] The same canonical delayed/failing executable appearing under multiple candidate sources is probed once.
- [x] A hung or noisy candidate is killed within its injected deadline, returns no installation, and cannot exceed the output cap.
- [x] The standard macOS Chrome app-bundle executable remains first in macOS platform defaults and a cold first probe can succeed without an explicit executable argument.
- [x] Diagnostic and public error tests prove arbitrary executable paths and version stdout are absent.

### Unit 3: Managed pointer-target activation and actionable hidden failure

**Files**:
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/page_lifecycle.rs`

**Checkpoint**: `epic-agent-browser-reliability-managed-session-lifecycle-pointer-activation`

```rust
// crates/krometrail-core/src/error.rs
pub enum ErrorCode {
    // existing codes...
    TargetHidden,
}

// crates/krometrail-cdp/src/control/mod.rs
pub(crate) struct BoundTarget {
    pub(crate) target_id: TargetId,
    pub(crate) browser_target_key: String,
    pub(crate) attachment_generation: u64,
    pub(crate) transport_session: TransportSessionId,
    pub(crate) visibility: TargetVisibility,
}

// crates/krometrail-cdp/src/control/interaction.rs
async fn prepare_pointer_target(
    &self,
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()>;

fn target_hidden_error(target_id: TargetId) -> KrometrailError;
```

**Implementation notes**:

- `bind_target` copies the opaque browser target key and supervisor visibility into `BoundTarget`; tests use opaque fixtures and diagnostics never log that raw key.
- In `execute_interaction_request`, run preparation after binding but before resolving a locator for `ActionCategory::{Pointer, DragDrop, Scroll}`. Visible targets take no extra CDP commands.
- For hidden/unknown targets, send `Target.activateTarget` in browser scope, then `Page.bringToFront` in the flat session scope, then boundedly evaluate `document.visibilityState`. Use the existing operation cancellation/generation race so shutdown or reconnect cannot continue input on a stale attachment.
- Accept only the literal `visible` result. A transport rejection, timeout, malformed result, or persistent `hidden` returns `target_hidden`, target context, `RetryAdvice::AfterRecovery`, and recovery “select or foreground the page, then retry the pointer operation.” Do not dispatch mouse events or manufacture an interaction ID on this pre-dispatch failure.
- Successful activation does not mutate `selected_target_key`; explicit-target operations must not silently change the caller's default page. Normal supervisor visibility events reconcile the eventual status.
- Keep element actionability separate: a CSS-hidden element remains `reference_not_actionable`; this unit concerns a hidden page target.

**Acceptance criteria**:

- [x] Pointer, drag/drop, and scroll against a supervisor-visible target emit no activation commands before their existing input sequence.
- [x] A hidden managed target emits browser activation, page foregrounding, a visibility recheck, and only then pointer input.
- [x] If the recheck is not `visible`, no pointer event is dispatched and the response uses `target_hidden` with target context and concrete recovery.
- [x] Activation preserves the selected Krometrail target and does not loosen stale-reference/actionability validation.
- [x] Deterministic transport tests assert command scope/order; a real-browser ignored qualification test hides a second page, clicks it without external macOS automation, and observes the expected page mutation.

### Unit 4: Rolling foundation and public contract projections

**Files**:
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`
- generated MCP/schema snapshots affected by `ErrorCode` and `BrowserStopOutcome` through the repository's existing generation/check path

```rust
// No parallel hand-written wire type. Existing serde/schema derivation remains authoritative.
```

**Implementation notes**:

- Update current assertions in place: hidden pointer targets are foregrounded or fail specifically; shutdown distinguishes degradation from remaining authority; discovery probes are bounded and sanitized.
- Regenerate checked-in schema artifacts through existing commands. Do not edit generated files by hand or add release-history prose.
- Ensure `managed_browser_closed_degraded` and `target_hidden` flow from core enums into MCP output/error schemas; no MCP-only duplicate enum is introduced.

**Acceptance criteria**:

- [x] Foundation docs describe the intended current contract without “previously”/migration prose.
- [x] Schema checks prove the two additive values are exposed and existing values remain unchanged.
- [x] Public docs do not recommend AppleScript or external application focus as normal recovery.

## Implementation Order

1. Implement the shutdown report and process-guard verification first; it is the highest-risk ownership boundary and fixes the false incomplete outcome without relying on interaction work.
2. Implement cold discovery policy and sanitized candidate diagnostics.
3. Implement pointer-target activation and the additive `target_hidden` error.
4. Regenerate contracts, roll foundation assertions forward, and run the complete integrated gate.

The three checkpoints are independent in the substrate graph (`depends_on: []`) because none consumes another checkpoint's code. The feature-level dependency on `durable-agent-diagnostics` remains the prerequisite for their causal events, and one feature owner should carry all checkpoints as a cohesive lifecycle bundle.

## Simplification

- Replace the shutdown-wide `failed` boolean with a report that directly represents degradation versus retained authority; remove the false `CaptureStreamState::Failed` completion check.
- Deduplicate canonical browser candidates before any potentially slow probe rather than only after successful probing.
- Reuse existing CDP activation commands and operation cancellation instead of adding a public focus tool or macOS automation path.
- Keep one `BrowserStopOutcome` enum and one `ErrorCode` registry-derived projection; no MCP-specific lifecycle result/error vocabulary.
- No standalone cleanup/refactor story is warranted; each deletion is cohesive with its regression fix.

## Testing

- **Discovery regression** (`launcher/discovery.rs`): injected fast/slow deadlines protect the cold-start bug, timeout bound, output cap, precedence, and one-probe-per-canonical-path behavior without a multi-second unit suite.
- **Pointer contract tests** (`control/tests.rs`): scripted CDP responses protect exact activation scope/order, no dispatch on persistent hidden state, cancellation, and visible-target zero-overhead behavior.
- **Real-browser qualification** (`tests/page_lifecycle.rs`, ignored/feature-gated): protects the integration assumption that `Target.activateTarget` plus `Page.bringToFront` changes a hidden managed renderer to visible enough for pointer input. It must not become a default platform-dependent gate.
- **Shutdown regression** (`session/mod.rs`): deterministic capture-failed, flush-failed, expired-after-release, and process-remains fixtures protect outcome classification, resource naming, cached idempotence, and aggregate deadline behavior.
- **Core/MCP contract tests**: assert exact serialization of all old and additive enum values and generated schemas. Existing clean/attached stop tests remain; do not duplicate them for every call site.
- **Integration evidence**: complete workspace fmt/check/test/clippy plus runtime `--version`, `--help`, and discovery-only `doctor`. A same-profile restart after degraded stop is the interface-level acceptance evidence for #5.
- **Test removal**: update the existing deadline-exhaustion assertion that equates budget exhaustion with `shutdown_incomplete`; retain its phase-order/deadline evidence and assert degraded success when no authority remains. No other useful test is removed.

## Risks

- **Riskiest assumption**: CDP activation and foregrounding may still be constrained by macOS window policy. The bounded visibility recheck prevents false success; the fallback is the explicit `target_hidden` result, not platform automation or unbounded waiting.
- **Process-group verification**: a process leader can exit while helpers remain, and PID/PGID reuse makes signaling dangerous. Preserve the existing ownership checks and never identify a remaining resource from an unverified PID alone.
- **Cold probe latency**: a 10-second cold budget could delay failure on a broken high-precedence executable. Pre-probe deduplication and the short PATH budget cap the cost; diagnostics show which source class consumed time.
- **Additive enum values**: generated clients may treat enums as closed. The new degraded stop value occurs only where old behavior was an error, and `target_hidden` replaces a generic error, so no previously successful wire response changes; schema generation and release notes must still call out the additions.
- **Cross-feature overlap**: capture-outcomes also touches post-operation outcomes and may inspect shutdown. This feature owns resource cleanup classification and the stop result; capture-outcomes owns capture-health reporting. Implementers should preserve that boundary to avoid competing result models.
- **Advisory posture**: no design-time advisory pass was run because the delegated task prohibited nested agents. The epic's standard fresh-context implementation/feature review remains required before release.

## Implementation notes

- Execution capability: highest available implementation posture; stable process/profile ownership and additive wire contracts warranted full causal verification.
- Review weight: standard, inherited from the epic/autopilot caller.
- Files changed: launcher discovery/process ownership, session shutdown/runtime/reconnect, control target binding/interaction preparation, core error/stop enums, and rolling foundation docs.
- Tests added/updated: delayed cold discovery, canonical failed-probe deduplication, aggregate shutdown deadline classification; existing enum round-trip tests cover additive stable values.
- Simplification: deduplicate candidates before probing; replace shutdown-wide failure outcome with resource truth plus quality; reuse existing CDP activation rather than adding a focus tool.
- Discrepancies from design: attached transport is always locally released by abort/drop in the current shutdown boundary, so remaining-resource accounting needs only managed process/profile classes.
- Adjacent issues parked: none.
- Integrated verification: `cargo fmt --all`; `cargo check --workspace --all-targets --locked`; focused launcher discovery and shutdown deadline tests.
