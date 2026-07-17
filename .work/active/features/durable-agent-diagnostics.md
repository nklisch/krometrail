---
id: durable-agent-diagnostics
kind: feature
stage: done
tags: [browser, storage, agent-ux, security]
parent: epic-agent-browser-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Durable agent diagnostics

## Brief

Give every Krometrail installation a durable, bounded, private diagnostic log that remains easy to locate regardless of the project directory from which the MCP server is used. The binary currently emits structured `tracing` events throughout lifecycle, CDP, capture, storage, and shutdown code but installs no tracing subscriber, so those events are normally discarded and agents can report only generic public errors and aggregate counters.

Initialize diagnostics before storage and browser composition, write sanitized structured events beneath the platform Krometrail data directory, and rotate or prune them under a fixed bound. Preserve the concrete failure stage and safe causal classification for capture, persistence, transport, observation, and shutdown failures. Do not log page content, screenshots, image bytes, form values, secrets, raw protocol payloads, or unredacted URLs.

Expose enough stable diagnostic context in MCP responses for an agent to find the correct evidence without guessing: a correlation identifier for failed/degraded operations, the active diagnostic-log path, and concise collection guidance. Update the Krometrail skill so an agent working in any repository knows when and how to inspect the bounded tail around that identifier, summarize the relevant sanitized events, and include version/platform/session/capture context in a later issue report without copying the entire log.

The file log is supplemental evidence. Stdout remains exclusively JSON-RPC, stderr remains useful for startup failures that occur before file logging is available, public structured errors remain actionable without log access, and diagnostic logging must never become a reason for browser control or MCP startup to fail.

## Strategic decisions

- **Availability**: create the bounded private diagnostic log by default so failures discovered after a walkthrough remain debuggable without prior opt-in.
- **Location**: place diagnostics under Krometrail's platform data directory, independent of the caller's current working directory, and expose the resolved path through the agent-facing surface.
- **Privacy**: retain operational metadata and sanitized causal classifications only; browser content, user input, media, raw CDP payloads, and unredacted URLs are outside the log contract.
- **Agent workflow**: expose correlation IDs and teach targeted excerpt collection; do not encourage agents to attach or paste whole logs.

## Simplification opportunity

Centralize diagnostic initialization, redaction, correlation, and retention at the composition root instead of adding issue-specific stderr messages or ad hoc files across adapters. Replace documentation that describes stderr as the only diagnostic destination while retaining stderr as the pre-initialization fallback.

## Design decisions

- **Format and location**: emit structured text through `tracing_subscriber` to `<data-directory>/diagnostics/krometrail.log`; never derive it from the caller's working directory.
- **Retention**: use one bounded writer with size-based rotation and a fixed generation count. Logging is best-effort; setup or write failure falls back to stderr and cannot prevent startup.
- **Correlation**: create an opaque request correlation ID at the MCP call boundary, enter a tracing span carrying it, and attach diagnostic metadata only to failed or degraded structured responses.
- **Privacy**: retain stable event names, error codes, operation/session/target identities, counts, durations, and failure stages; exclude raw CDP payloads, browser/form content, media, secrets, and full URLs.

## Architectural choice

Initialize a process-owned diagnostic guard before `build_runtime`, then inject an immutable `DiagnosticContext` into MCP composition. This keeps file ownership and fallback at the executable boundary while making the correlation path available to the adapter. A global mutable diagnostics API and issue-specific files were rejected because they obscure lifetime, isolation, and discovery.

## Implementation units

### Unit 1: bounded process diagnostics

**Files**: `src/diagnostics.rs`, `src/main.rs`, `src/app.rs`, `Cargo.toml`

```rust
pub(crate) struct DiagnosticRuntime {
    context: krometrail_mcp::DiagnosticContext,
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

pub(crate) fn initialize(data_directory: &Path)
    -> Result<Option<DiagnosticRuntime>, DiagnosticInitError>;
```

Implement a redaction-safe formatter and size-bounded rotating writer under `diagnostics/`. Install the subscriber before storage/browser construction. A sanitized stderr warning is the only initialization fallback. Preserve the guard for process lifetime and never write diagnostics to stdout.

**Acceptance criteria**:
- [x] `mcp` creates a log under the Krometrail data directory regardless of current directory.
- [x] Rotation enforces a fixed aggregate bound and startup succeeds when diagnostics are unavailable.
- [x] JSON-RPC stdout contains no log records.

### Unit 2: request correlation and response projection

**Files**: `crates/krometrail-mcp/src/config.rs`, `crates/krometrail-mcp/src/server.rs`, `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/lib.rs`, `src/app.rs`

```rust
#[derive(Clone, Debug)]
pub struct DiagnosticContext { pub log_path: Option<PathBuf> }

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ResponseDiagnostics {
    pub correlation_id: String,
    pub log_path: Option<String>,
}

pub struct ToolResponse {
    // existing fields
    pub diagnostics: Option<ResponseDiagnostics>,
}
```

Generate a UUID for every tool/resource request, enter a span with the stable route name, and enrich failed/degraded tool envelopes. Summary text stays actionable without log access. Resource failures carry the ID in safe error data where the protocol shape permits it.

**Acceptance criteria**:
- [x] Failed/degraded calls expose a correlation ID and active log path; successful calls add no diagnostic noise.
- [x] The same ID appears in request-bound events.
- [x] Existing response fields retain their serialized names and meaning.

### Unit 3: causal event audit

**Files**: `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/session/shutdown.rs`, `crates/krometrail-cdp/src/launcher/discovery.rs`, relevant tests

Add sanitized failure-stage events where errors are discarded or collapsed into counters. Use stable names and `error_code`/`failure_stage`; avoid arbitrary `Debug` formatting when errors may contain endpoints or payloads.

**Acceptance criteria**:
- [x] Decode, persistence, event-stream, observation, discovery-probe, and shutdown failures retain a safe stage.
- [x] A source audit prevents raw payload or sensitive field logging in capture/interaction hot paths.

## Implementation order

1. Bounded writer and process initialization.
2. Inject context and correlate MCP requests.
3. Audit known blind causal boundaries.

## Simplification

- One process destination replaces discarded tracing and future per-issue stderr workarounds.
- Correlation metadata remains absent from successful envelopes.

## Testing

- Unit-test size rotation, unavailable-directory fallback, and current-directory independence.
- MCP contract tests assert metadata only on degraded/failed responses and correlate it to captured tracing output.
- Source-level privacy checks guard hot paths from raw payload and sensitive-field logging.

## Risks

- A non-blocking writer can drop records during a crash; expose dropped-event warnings and prefer bounded loss over blocking control.
- Existing arbitrary error formatting may contain endpoints; audit it before enabling the production filter.

## Implementation notes

- Execution capability: GPT-5.6 Sol at high effort; this is a stable public response and privacy boundary spanning the process composition root, MCP adapter, and CDP causal boundaries.
- Review weight: standard, from the autopilot caller.
- Files changed: `src/diagnostics.rs`, `src/main.rs`, `src/app.rs`, root Cargo metadata, MCP configuration/server/response exports, and targeted CDP capture, discovery, and shutdown paths.
- Tests added: bounded rotation and private-file behavior, configured-root path behavior, unavailable-destination fallback, failed-versus-successful MCP diagnostic projection, and a capture-source privacy audit.
- Simplification: one process-owned nonblocking writer and one injected diagnostic context replace discarded tracing and issue-specific stderr/log paths; successful MCP envelopes omit diagnostic metadata.
- Discrepancies from design: the rotating writer is Krometrail-owned and fed through `tracing_appender`'s bounded nonblocking channel because `tracing_appender`'s built-in rolling policy is time-based rather than size-bounded. Diagnostic setup returns `None` when another subscriber is already installed, preserving embedding/test isolation.
- Privacy audit: the production filter enables only info-or-higher Krometrail crate targets; audited events carry stable codes, stages, identities, counts, and durations, never raw errors, payloads, URLs, page content, form values, or media.
- Adjacent issues parked: none.

## Review record

- Effective weight: standard; pass: 1; verdict: approve after fixes.
- Findings fixed: diagnostics fallback is rooted in the system temporary directory instead of the caller's working directory; runtime and MCP configuration docs now describe the private log destination and stderr contract.
- Verification: diagnostic rotation/path/privacy tests, MCP correlation projection tests, full workspace tests, strict clippy, and documentation build passed.

## Verification

- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --bin krometrail diagnostics::tests --locked` — 3 passed.
- `cargo test -p krometrail-mcp diagnostics_are_added_only_to_failed_or_degraded_tool_envelopes --locked` — passed.
- `cargo test -p krometrail-cdp status_and_gap_serialization_are_privacy_safe --locked` — passed.
- `cargo test -p krometrail-cdp launcher::discovery::tests --locked` — passed.
- A built `mcp` process with stdin at EOF exited successfully with zero stdout/stderr bytes while writing the canonical diagnostics log beneath an isolated `KROMETRAIL_DATA_DIR`.
