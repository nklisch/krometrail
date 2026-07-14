---
id: refactor-remove-unused-progressive-response-control
kind: story
stage: done
tags: [refactor, agent-ux]
parent: null
depends_on: [epic-temporal-debugging-workflow-mcp-investigation-surface]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Remove unused progressive response-mapper control inputs

## Brief

`crates/krometrail-mcp/src/response.rs:664-670` declares
`map_progressive_result` with `ProgressiveEvidence`, deadline, and cancellation
arguments, but the mapper performs no I/O and does not use them. The function
ends with `let _ = (progressive, deadline, cancellation);` at lines 737-740 to
suppress that accidental seam. Its only caller is
`crates/krometrail-mcp/src/registry.rs:391-400`, which passes a live evidence
service and request budget solely because the initial projection design left
room for hypothetical future byte-bearing result families.

Make the mapper synchronous and accept only the tool name and typed result.
Remove the unused arguments, placeholder tuple, and caller-side `.await` while
leaving the actual progressive result projection and resource metadata logic
unchanged.

**Source lens**: elimination first / dead control seam / speculative scaffolding

**Rationale**: removes a false dependency from a pure response projection
boundary. Resource reads and bundle inline retrieval retain their real
progressive/deadline paths; the mapper no longer advertises or reserves a
control responsibility it does not perform.

**Black-box classification**: pure refactor. All progressive result JSON,
resource links, inline-source limits, response statuses, errors, and tool
routing remain identical.

## Current State

```rust
pub(crate) async fn map_progressive_result(
    tool: &str,
    result: ProgressiveEvidenceResult,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
) -> Result<MappedResult, ResponseInvariantError> {
    // ...pure match...
    let _ = (progressive, deadline, cancellation);
    Ok(mapped(...))
}
```

The registry call awaits this function even though its body contains no await
point and the three control arguments have no effect.

## Target State

A synchronous private mapper with signature:

```rust
fn map_progressive_result(
    tool: &str,
    result: ProgressiveEvidenceResult,
) -> Result<MappedResult, ResponseInvariantError>
```

The registry calls it directly after the progressive port future has completed.
The real deadline/cancellation context remains on the port invocation and on
`map_temporal_bundle_result`'s inline artifact read.

## Acceptance Criteria

- [ ] `map_progressive_result` has no unused progressive/deadline/cancellation parameters and is not async.
- [ ] Its sole caller invokes it synchronously; the placeholder tuple and speculative-control comment are gone.
- [ ] Every progressive result variant retains the same structured projection, resource-link validation, inline image behavior, status, error, and summary output.
- [ ] No cancellation/deadline behavior is removed from progressive execution or bundle inline retrieval.
- [ ] `cargo fmt --all -- --check`, locked workspace check/test, and Clippy with `-D warnings` pass.

## Risk and Rollback

**Risk**: Low. The removed values are provably unused; the main risk is
accidentally editing the adjacent temporal bundle mapper, which must remain
async because it reads the inline artifact.

**Rollback**: Revert the implementation commit to restore the async mapper
signature and its caller arguments. No protocol, storage, or compatibility
rollback is required.

## Discovery Notes

- **Scope**: temporal MCP routing/response/resource implementation from commits
  `6b5776b` through `245fb1f`; verified by direct reads of
  `crates/krometrail-mcp/src/{registry,response}.rs`.
- **Dispatch**: direct-read only; no exploratory agent or peer review was used.
- **Project conventions**: no project refactor-convention catalog exists; the
  built-in code-economy and elimination lenses were applied. This is not a
  generic MCP handler abstraction.
- `.work/bin/work-view` and current epic/feature stages were preserved.

## Implementation notes

- Execution capability: direct inline implementation; the pure mapper and its sole registry caller form one bounded private seam.
- Review weight: standard, with the bounded standalone-story review still pending.
- Files changed: `crates/krometrail-mcp/src/response.rs` and `crates/krometrail-mcp/src/registry.rs`.
- Tests added/removed: none; existing progressive MCP response and resource tests remain the equivalence evidence.
- Simplification: made `map_progressive_result` synchronous, removed its unused evidence/deadline/cancellation inputs and placeholder tuple, and removed the caller-side await while leaving execution and temporal-bundle inline retrieval control paths unchanged.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Focused verification (Rust 1.85.0, locked): `cargo fmt --all -- --check`; `cargo check -p krometrail-mcp --locked --all-targets`; `cargo test -p krometrail-mcp --locked` (19 passed); `cargo clippy -p krometrail-mcp --locked --all-targets -- -D warnings`.

## Review decision

**Approved.** A fresh-context `openai-codex/gpt-5.6-luna` bounded standalone-story review confirmed the mapper was a pure projection, its sole caller remains correct, progressive execution retains deadline/cancellation/geometry control, and bundle inline retrieval remains async and controlled. JSON, resource, image, status, and error behavior are unchanged. Full Rust 1.85 workspace gates passed. The story advances to `done`.
