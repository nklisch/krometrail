---
id: epic-agent-surface-simplification-optional-batch-evidence
kind: feature
stage: done
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-response-detail]
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Optional batch step screenshot evidence

## Brief

Make batch step screenshot evidence genuinely optional across the core result, CDP execution, and MCP projection. When screenshots are disabled or a step is never attempted, omit the screenshot field. When screenshots were requested and capture failed, retain the structured unavailable/error evidence. Prove disabled mode issues no per-step screenshot acquisition.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: canonical batch-semantics consumer of the new response shape

## Simplification opportunity

Delete fabricated `Unsupported` screenshot observations, placeholder helpers, imports, and repeated branches. Model absence directly instead of teaching the projector to hide a false domain outcome.

## Foundation references

- `docs/SPEC.md` — Batching
- `docs/ARCHITECTURE.md` — Interaction Execution and MCP Boundary

## Design decisions

- **Absence representation**: `None` means screenshot evidence was not requested or the step never executed; `Some(Unavailable)` is reserved for a requested screenshot that could not be acquired.
- **Wire shape**: MCP omits `steps[].screenshot` when the domain value is absent rather than serializing `null`.
- **Acquisition control**: retain `include_step_screenshots`; it governs an actual additional CDP observation and is not merely presentation detail.

## Architectural choice

Three options were considered. A response-projector check could hide the fabricated unsupported object but would leave the canonical domain result false. Adding a third `ObservationPart::NotRequested` variant would spread presentation-specific absence through every observation consumer. The chosen design makes only `BatchStepResult.screenshot` optional: absence is native to the batch contract, while the existing `ObservationPart` continues to distinguish available from requested-but-unavailable evidence.

The riskiest unit is the constructor invariant because skipped, failed, and successful steps share one result type. The constructor permits `None` for every status, permits `Some` only as truthful acquired/requested evidence, and keeps the existing prohibition on available screenshots for skipped steps.

## Implementation Units

### Unit 1: Optional canonical batch evidence

**File**: `crates/krometrail-core/src/browser/batch.rs`

```rust
pub struct BatchStepResult {
    // existing fields
    pub screenshot: Option<ObservationPart<EncodedScreenshot>>,
}

impl BatchStepResult {
    pub fn new(
        // existing arguments
        screenshot: Option<ObservationPart<EncodedScreenshot>>,
    ) -> Result<Self>;
}
```

**Implementation notes**:
- Preserve all existing status/timing/result invariants.
- Reject `Some(Available(_))` for skipped steps; `None` is the normal skipped value.
- Do not introduce a compatibility constructor or deprecated field.

**Acceptance criteria**:
- [ ] A disabled, skipped, or never-attempted screenshot is represented by `None` without a fabricated error.
- [ ] A requested screenshot failure remains `Some(ObservationPart::Unavailable(error))`.

### Unit 2: Acquire only requested step screenshots

**File**: `crates/krometrail-cdp/src/control/batch.rs`

```rust
let mut screenshot = result
    .as_ref()
    .and_then(existing_screenshot)
    .map(Some)
    .unwrap_or(None);

if request.options.include_step_screenshots && screenshot.is_none() {
    screenshot = Some(capture_step_screenshot(/* existing context */).await);
}
```

**Implementation notes**:
- Initialize disabled/skipped/target-unavailable branches with `None`.
- Preserve an operation's existing live screenshot when present; no extra CDP call is needed.
- Delete `unavailable_screenshot` and the imports used only to fabricate unsupported results.

**Acceptance criteria**:
- [ ] `include_step_screenshots: false` sends no per-step `Page.captureScreenshot` command.
- [ ] Requested screenshot success and failure remain truthful and associated with the correct step.

### Unit 3: Omit absent screenshot fields from MCP

**File**: `crates/krometrail-mcp/src/response.rs`

```rust
fn project_batch_step_screenshot(
    screenshot: Option<ObservationPart<EncodedScreenshot>>,
    images: &mut Vec<EncodedMcpImage>,
    step_index: u32,
) -> Option<serde_json::Value>;
```

**Implementation notes**:
- Build each step as a JSON object and insert `screenshot` only when the helper returns `Some`.
- Requested unavailable evidence remains a structured unavailable object.
- Available screenshot metadata and optional inline image content retain the shared response-detail policy.

**Acceptance criteria**:
- [ ] Successful disabled batches contain no `steps[].screenshot` key.
- [ ] Requested failure emits exactly one structured screenshot-unavailable value and no inline image.

## Implementation Order

1. Change the core invariant and fixtures.
2. Remove fabricated CDP observations and prove acquisition absence.
3. Update MCP mapping and protocol tests.

## Simplification

- Delete `unavailable_screenshot`, its `NonEmptyText`/`Unsupported` dependencies, and three placeholder construction paths.
- Remove tests that assert unsupported placeholders and replace them with absence and requested-failure assertions.

## Testing

- Extend `crates/krometrail-cdp/tests/waits_and_batches.rs` with transport-command accounting for disabled screenshots and preserve the requested-screenshot qualification.
- Update focused core constructor tests for `None`, requested unavailable, and skipped available rejection.
- Update `crates/krometrail-mcp/src/response.rs` batch fixtures to assert key absence and requested failure serialization.

## Risks

Hand-built JSON can accidentally serialize `null`; tests must assert key absence. Existing post-action screenshots must be reused without causing an extra capture command.

## Review

Approved in the single standard fresh-context pass with no blockers. The reviewer verified truthful `Option` semantics, disabled transport-command absence, existing evidence reuse, requested failure preservation, omitted MCP keys, inline behavior, focused regressions, and deletion of fabricated unsupported evidence.

## Implementation notes

- Execution capability: raised — the change crosses the canonical core result, CDP acquisition, and MCP projection, but remains one cohesive contract slice.
- Review weight: standard (autopilot caller).
- Files changed: `crates/krometrail-core/src/browser/batch.rs`, `crates/krometrail-cdp/src/control/batch.rs`, `crates/krometrail-cdp/tests/waits_and_batches.rs`, `crates/krometrail-mcp/src/response.rs`.
- Tests added/removed: added the core absence/requested-failure distinction, CDP command accounting for disabled screenshots, and MCP key-absence/requested-failure assertions; removed placeholder expectations through the same focused coverage.
- Simplification: deleted the fabricated `Unsupported` screenshot helper and made disabled/skipped evidence native absence; MCP now inserts the field only when evidence exists.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo check --workspace --all-targets --locked`
- `cargo test -p krometrail-core browser::batch::tests --locked`
- `cargo test -p krometrail-cdp --test waits_and_batches batch_stop_and_continue_policies_preserve_failed_wait_results --locked`
- `cargo test -p krometrail-cdp --test waits_and_batches requested_step_screenshot_uses_standalone_path_before_one_final_observation --locked`
- `cargo test -p krometrail-mcp response::tests::degradation_wait_timeout_page_anchor_and_batch_failure_remain_distinct --locked`
