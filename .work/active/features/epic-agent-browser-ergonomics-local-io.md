---
id: epic-agent-browser-ergonomics-local-io
kind: feature
stage: done
tags: [agent-ux, browser, security]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Explicit clipboard and download workflows

## Brief

Add bounded explicit clipboard read/write operations and a managed-download lifecycle with completion metadata, cancellation, and canonical local resource access. Define managed-versus-attached authority, a session-owned download directory, cleanup/retention behavior, and focus or permission failures precisely. Content and local paths never enter browser-event evidence, diagnostics, or ordinary status.

The feature excludes arbitrary filesystem destinations, implicit clipboard access, download uploads, response-body capture, and silent permission escalation. Unsupported platforms or externally owned sessions fail with stable recovery guidance.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent local I/O capability with elevated privacy and ownership risk

## Simplification opportunity

Use the existing operation registry for explicit mutability, browser-event pipeline for bounded lifecycle signals, and canonical resource layer for completed bytes. Keep paths and clipboard content out of shared diagnostics and timeline payloads.

## Foundation references

- `docs/SPEC.md` — Browser-Control Surface and Local Data and Telemetry
- `docs/ARCHITECTURE.md` — MCP Boundary, Failure Isolation, and Observability

## Design decisions

- **Ownership**: clipboard and download tools are supported only for Krometrail-managed browser sessions. Attached sessions fail before access with `unsupported` and guidance to start a managed profile; explicit attachment does not confer host clipboard or filesystem authority.
- **Clipboard permission**: an explicit tool call authorizes the requested read/write, but Krometrail does not grant, reset, or persist browser clipboard permissions. Chrome's current secure-context, visibility, focus, and permission policy remains authoritative and failures name the required recovery.
- **Clipboard evidence**: clipboard text appears only in the explicit read result or write request. Timeline records, interaction parameters, ordinary status, logs, and diagnostics contain operation kind plus byte count only.
- **Download storage**: managed Chrome writes `allowAndName` downloads into a private session directory using Chrome's opaque GUID filename. Krometrail exposes a validated display filename only in explicit download tools and never uses it as a path.
- **Download bounds**: at most 32 tracked downloads and 64 MiB per file. Crossing a bound cancels/removes the file and returns `resource_limit_exceeded`; it never leaves an unbounded partial artifact.
- **Resource lifetime**: completed download resources are readable only while that browser session is active. `stop_browser`, process shutdown, failed launch cleanup, or session loss cancels active downloads and removes the complete session directory; this capability does not make downloads retained temporal evidence.
- **Focus**: local-I/O tools obey the existing focus policy and never activate a page. A hidden or unfocused page returns a concrete focus/permission failure instead of stealing focus.
- **Mockups**: no UI surface; MCP contracts and skill guidance only.
- **Dispatch**: direct-read design over the operation registry, session ownership, CDP event/command paths, launcher cleanup, canonical resources, and composition root.

## Architectural choice

Three approaches were considered. OS clipboard and arbitrary filesystem adapters would be reliable but would escape the selected browser/session authority. A page-script-only implementation without lifecycle ownership would be small but could not safely contain downloads or provide race-free completion. A retained-store implementation would keep files after browser stop but would add migrations, disk-budget semantics, and deletion coupling that are not needed for browser-control parity. The chosen design adds a managed local-I/O authority inside the production browser session: explicit page clipboard operations use Chrome's existing permission state, while a session-owned download tracker configures a private bounded directory, consumes browser-level lifecycle events, and serves active-session canonical resources through the existing MCP owner.

The trickiest unit is download completion and cleanup. Chrome events, file publication, cancellation, session shutdown, and resource reads can race. One serialized `ManagedDownloadAuthority` owns the GUID map and state transitions; a completed file becomes readable only after Chrome reports completion and Krometrail verifies a regular bounded file under the canonical session root.

## Implementation Units

### Unit 1: Explicit managed-page clipboard operations

**Files**: `crates/krometrail-core/src/browser/local_io.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-cdp/src/control/clipboard.rs`, `crates/krometrail-cdp/src/control/mod.rs`, `crates/krometrail-cdp/src/session/operations.rs`, `crates/krometrail-mcp/src/response.rs`
**Story**: `epic-agent-browser-ergonomics-local-io-clipboard`

```rust
pub const MAX_CLIPBOARD_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReadClipboardRequest { #[serde(default)] pub target: PageSelection }

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ClipboardRead {
    pub target_id: TargetId,
    pub text: String,
    pub utf8_bytes: u64,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteClipboardRequest {
    #[serde(default)] pub target: PageSelection,
    pub text: String,
}

impl Debug for WriteClipboardRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result; // target + UTF-8 length only
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardWriteResult {
    pub interaction: InteractionAnchor,
    pub utf8_bytes: u64,
    pub outcome: PageOperationOutcome,
    pub observation: ObservationPart<LiveObservation>,
}
```

**Implementation notes**:
- Register `read_clipboard` as page-scoped/read-only/requested-only and `write_clipboard` as page-scoped/state-changing/live-observation. Both are non-batchable so clipboard content cannot be copied into batch results or nested request diagnostics.
- Reject attached ownership before binding a target. Require a visible selected page; do not call `Target.activateTarget` or `Page.bringToFront`, including under foreground policy.
- Invoke fixed adapter-owned functions through `Runtime.callFunctionOn`; pass write text as a CDP argument value, never concatenate it into JavaScript. Use `navigator.clipboard.readText()`/`writeText()` only in secure page context and await the promise.
- Validate UTF-8 byte length before sending and after reading. Map focus, secure-context, unavailable API, and denied permission failures to stable `unsupported`/`interaction_failed` errors with distinct recovery; never issue `Browser.grantPermissions`, `Browser.setPermission`, or `Browser.resetPermissions`.
- Implement manual redacted `Debug` for content-bearing request/result types. Interaction evidence sanitization records `utf8_bytes`, never text.

**Acceptance criteria**:
- [ ] A permitted, focused managed page can explicitly read and write clipboard text up to the bound, with the write returning ordinary post-operation evidence.
- [ ] Attached, hidden/unfocused, insecure, denied, unavailable, and oversized cases fail before unintended mutation and provide actionable recovery without focus activation or permission changes.
- [ ] Sentinel clipboard values are absent from timeline records, browser events, ordinary status, tracing output, correlation diagnostics, and `Debug` formatting.

### Unit 2: Serialized bounded download authority

**Files**: `crates/krometrail-core/src/browser/downloads.rs`, `crates/krometrail-core/src/browser/operation.rs`, `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-cdp/src/session/downloads.rs`, `crates/krometrail-cdp/src/session/runtime.rs`, `crates/krometrail-cdp/src/session/shutdown.rs`, `crates/krometrail-cdp/src/launcher/profile.rs`, `src/app.rs`
**Story**: `epic-agent-browser-ergonomics-local-io-download-authority`

```rust
pub const MAX_MANAGED_DOWNLOADS: usize = 32;
pub const MAX_MANAGED_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

define_uuid_id!(DownloadId);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct DownloadSequence(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState { InProgress, Completed, Cancelled, Failed, Rejected }

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManagedDownload {
    pub id: DownloadId,
    pub sequence: DownloadSequence,
    pub target_id: Option<TargetId>,
    pub state: DownloadState,
    pub suggested_filename: DownloadDisplayName,
    pub source_url: SanitizedUrl,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resource_uri: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DownloadInventory {
    pub session_id: SessionId,
    pub cursor: DownloadSequence,
    pub downloads: Vec<ManagedDownload>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListDownloadsRequest {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaitForDownloadRequest {
    pub after: DownloadSequence,
    pub download_id: Option<DownloadId>,
    pub terminal: bool,
    pub timeout: DurationMillis,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancelDownloadRequest { pub download_id: DownloadId }

pub struct ManagedDownloadConfig {
    pub root: PathBuf,
    pub max_downloads: usize,
    pub max_file_bytes: u64,
}

pub(crate) struct ManagedDownloadAuthority {
    config: ManagedDownloadConfig,
    // serialized GUID -> validated entry state; raw GUID/path never leaves this module
}
```

**Implementation notes**:
- Configure only managed sessions with `Browser.setDownloadBehavior { behavior: "allowAndName", downloadPath, eventsEnabled: true }` after the private `0700` session directory is created and canonicalized under the configured download root. Attached sessions never receive this command.
- Subscribe to `Browser.downloadWillBegin` and `Browser.downloadProgress` before enabling behavior. Allocate Krometrail `DownloadId`/sequence values and map frame IDs to a supervised target only when proven; `None` is preferable to guessing.
- Sanitize the explicit display filename to a bounded basename with separators/control characters removed. The on-disk name is always the Chrome GUID. URLs use `SanitizedUrl`.
- One reducer accepts begin/progress/cancel/shutdown inputs and emits CDP cancel, bounded lifecycle-event, publish, and delete effects. Progress beyond 64 MiB or a 33rd download transitions to `Rejected`, cancels through `Browser.cancelDownload`, and deletes partial bytes.
- A `Completed` transition is acknowledged only after the GUID path is a non-symlink regular file, canonicalizes beneath the session root, and its exact size is within the configured bound. Resource publication is then immutable for that session.
- `list_downloads`, `wait_for_download`, and `cancel_download` are browser-scoped, explicit, non-batchable operations. Wait uses the same cursor-before-action pattern as page waits and checks retained reducer state before subscribing.
- Browser-event evidence receives only `DownloadId`, state, target if known, and byte counts. Suggested names, source URLs, GUIDs, paths, and contents never enter the timeline or logs.
- Shutdown first closes resource admission, cancels active GUIDs, drains bounded tracker work, then recursively removes only its canonical session root. Cleanup failure contributes to the existing degraded/shutdown-incomplete ownership result.

**Acceptance criteria**:
- [x] A managed download begun between list and wait is found race-free, progresses deterministically, and becomes `Completed` only when its verified file is readable.
- [x] Cancellation, browser failure, over-count, over-size, symlink/non-regular file, and shutdown races terminate once and leave no readable partial resource.
- [x] Attached sessions fail before changing browser download behavior or touching a download directory.
- [x] Filenames, raw URLs, GUIDs, filesystem paths, and bytes are absent from browser events, status, tracing, and diagnostics.
- [x] Stop and failed-start cleanup remove the session directory without deleting sibling sessions or the reusable browser profile.

### Unit 3: Active-session canonical download resources and skill guidance

**Files**: `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-mcp/src/session.rs`, `crates/krometrail-mcp/src/resources.rs`, `crates/krometrail-mcp/src/registry.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`
**Story**: `epic-agent-browser-ergonomics-local-io-resource-surface`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadManagedDownloadRequest {
    pub session_id: SessionId,
    pub download_id: DownloadId,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedDownloadRead {
    pub session_id: SessionId,
    pub download_id: DownloadId,
    pub media_type: NonEmptyText,
    pub bytes: Vec<u8>,
}

pub trait BrowserSessionPort: Send + Sync {
    // existing methods
    fn read_managed_download(&self, request: ReadManagedDownloadRequest)
        -> PortFuture<'_, Result<ManagedDownloadRead>>;
}
```

Canonical URI: `krometrail://local/{session}/downloads/{download}`.

**Implementation notes**:
- Add one registry-declared `application/octet-stream` resource template and strict manual URI parser. IDs must use canonical UUID spelling; percent escapes, query, fragment, backslash, and extra segments are rejected.
- `BrowserSessionOwner::read_managed_download` forwards only when the requested session is the active session. A stopped/replaced session returns resource-not-found; no historical lookup or path fallback exists.
- The session authority rechecks state `Completed`, file identity, canonical containment, and exact size on every read, then performs a bounded read through its private path. MCP maps bytes to one blob resource and never includes a path.
- Add plugin guidance: clipboard calls are explicit and permission/focus preserving; download cursors should be captured before triggering a download; completed resources are local and expire on `stop_browser`; attached sessions are unsupported.

**Acceptance criteria**:
- [x] A completed active-session download has one canonical resource URI whose read returns exact bytes and declared size up to 64 MiB.
- [x] In-progress, cancelled, rejected, oversized, mismatched-session, malformed-URI, and post-stop reads fail without path disclosure.
- [x] Schema/resource registry tests and plugin static checks prove the tool/resource surface and lifetime guidance are complete.

## Implementation order

1. Clipboard contracts and managed-ownership guard.
2. Download directory/event reducer, bounded publication, cancellation, and shutdown cleanup.
3. Active-session resource reads and agent skill guidance.
4. Generated schema/resource fixtures plus real-browser qualification on Linux and macOS.

## Simplification

- Reuse `BrowserSessionOwner`, the browser operation registry, interaction evidence, cancellation, stable error mapping, and canonical resource machinery.
- Keep local I/O session-scoped; do not add SQLite migrations, retention-budget accounting, arbitrary destination configuration, or a second persistent download catalog.
- Use Chrome's GUID filename directly and delete display-name-to-path mapping, collision handling, and extension-based trust decisions from the design.
- No existing test is removed; extend lifecycle/fixture harnesses rather than introducing a separate browser runtime.

## Testing

- Core contract tests protect byte/count limits, filename sanitization, state transitions, serde/schema generation, cursor ordering, and redacted `Debug`.
- Scripted CDP tests protect no-permission-escalation clipboard calls, attached-session rejection, browser-level subscription-before-enable ordering, cancel behavior, and GUID/path non-projection.
- Filesystem boundary tests use a private temp root to protect canonical containment, symlink/non-regular rejection, exact-size reads, partial deletion, sibling isolation, and idempotent cleanup.
- MCP interface tests protect operation registry membership, canonical URI grammar, active-session lifetime, blob size/content, and stable visible errors.
- Real Chrome qualification covers permitted clipboard access when the fixture grants it, denied/unfocused guidance, successful download/read/cancel, oversize cancellation, and stop cleanup. Platform-specific permission failures are accepted only when they match the declared stable recovery contract.
- Privacy regression tests use sentinels in clipboard text, filename, URL, GUID, path, and bytes and assert they never appear in events, status, logs, diagnostics, or non-explicit responses.

## Risks

- **Riskiest assumption**: current Chrome builds consistently support browser-level download events with `allowAndName` on the supported managed-launch path. If qualification fails on one supported platform, retain clipboard delivery and return the download portion to drafting; do not poll arbitrary directories or expose paths as a fallback.
- Clipboard availability depends on OS/browser policy and focus. The tool guarantees explicitness and clear failure, not silent permission bypass or universal success.
- Chrome can report completion before filesystem visibility on some filesystems. Publication performs a bounded post-event verification; timeout fails the download and cleans it rather than publishing uncertain bytes.
- Session-lifetime resources may surprise agents accustomed to retained evidence. Every download result and the skill state that `stop_browser` invalidates the URI.

## Implementation summary

- Delivered all three checkpoints: explicit managed-page clipboard operations (`41438a0`), the bounded managed-download authority (`c7429b0`), and active-session canonical download resources (`0e7e33d`).
- The operation registry remains the single tool/schema authority. Clipboard content and download bytes stay out of diagnostics, status, browser events, and interaction parameters; expanded local I/O is explicit while ordinary responses retain compact defaults.
- Managed launch now owns one private session download directory and browser-level event authority. Attached sessions fail before clipboard/download access, and stop/session loss removes download admission and local resources.
- The installed skill documents the lower-cost cursor-first workflow, explicit expansion points, focus/permission preservation, local-only bytes, and resource lifetime.

## Verification

- Core browser contracts: 153 passing before one unrelated browser-context batchability assertion exposed by concurrent integration; the context lane owns that four-variant test-list correction.
- CDP: clipboard scripted tests 2/2; download reducer/filesystem tests 4/4; all-target check green.
- MCP: full suite 61/61, including strict URI grammar, resource registry, exact blob bytes, and post-stop invalidation.
- Workspace: `cargo check --workspace --all-targets --locked` green.
- Real managed Chrome: launched a temporary profile through the built MCP server, captured a pre-action download cursor, clicked a data-URL download, observed terminal completion, read exact `hello krometrail` bytes through the canonical resource, stopped the browser, and confirmed the resource was invalidated.

## Review

Standard review initially requested changes for terminal/effect races, over-count rejection visibility, reconnect restoration, supervisor-blocking waits, missing privacy-safe lifecycle evidence, clipboard error classification, and redacted diagnostics. All findings were accepted and repaired without broadening the feature:

- `0861b18` serializes lifecycle effects, fences stale transport generations, makes terminal states immutable, and bounds visible overflow rejection.
- `8f65d64` moves download waits outside the supervisor, honors caller/session cancellation, and restores subscriptions plus download behavior on reconnect.
- `2960b45` preserves transport disconnect classification, distinguishes unsupported clipboard contexts from recoverable interaction failures, and redacts local-I/O `Debug` output.
- `74e6de8` moves bounded artifact reads off the async runtime and cleans partial private roots on setup failure.
- `d435925` persists clipboard byte-count-only evidence and emits privacy-safe download lifecycle events without names, URLs, GUIDs, paths, resource URIs, or bytes.
- `d3427e4` covers bounded overflow, stable resource-limit signaling, cancellation, and privacy-safe lifecycle event projection.

Final verification: download authority tests 7/7, clipboard tests 3/3, local-I/O core tests 3/3, evidence tests 2/2, and `cargo clippy -p krometrail-core -p krometrail-cdp --all-targets --locked -- -D warnings` pass. The earlier MCP 61/61, workspace all-target check, and real managed-Chrome resource-lifetime qualification remain valid. Verdict: pass; feature moved from `review` to `done`.
