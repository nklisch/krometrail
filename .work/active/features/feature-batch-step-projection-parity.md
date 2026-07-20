---
id: feature-batch-step-projection-parity
kind: feature
stage: review
tags: [bug, agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Batch step result projection parity

## Brief

Batch step results bypass the detail-tiered root projection that standalone
tools apply, so a batch containing `snapshot_page` (or `inspect_page`) on a
large page serializes the full raw node tree and blows the host token cap. A
batch [navigate → scroll → snapshot_page anchor=viewport] on Wikipedia "Web
browser" (4197 AX nodes) returned 828KB — step[2].result was the complete raw
node array (~813KB) while the identical standalone call projects the bounded
48-target viewport ranking (~13KB). The batch's own `final_observation` in the
same response was correctly bounded; only the per-step embedding is
unprojected.

## Root cause

`project_response(tool, ...)` (crates/krometrail-mcp/src/response.rs:1708) is
what converts a raw `snapshot_page` result into the concise `targets`
projection and a raw `inspect_page` result into concise page state — it runs
*after* `project_operation` in the standalone path
(`map_operation_result_with_capture_and_novelty`, line 489/492). The batch
path calls `project_batch_step` → `project_operation` directly and never
applies the tool-specific root projection, so `SnapshotPage` stays
`serializable(*value)` (raw `nodes`) and `InspectPage` stays full page state.
Every other operation projects identically in both paths; only the two tools
with a `project_response` special-case regress inside batch.

## Design decision
- **Route step results through one shared tool-specific projector**: extract
  the `snapshot_page`/`inspect_page` root-projection dispatch from
  `project_response` into a helper keyed by operation name, and call it from
  `project_batch_step` using the step's own `operation`. The batch inherits the
  batch request's `detail`, exactly as `final_observation` already does. No
  wire or domain change; canonical-result-projection is restored for step
  results.

## Implementation Units

### Unit 1: Shared tool root projector
**File**: `crates/krometrail-mcp/src/response.rs`

```rust
// New helper, called by both project_response and project_batch_step.
fn project_tool_root(
    operation: &str,           // "snapshot_page" | "inspect_page" | ...
    result: &mut Value,
    response: ResponseRequest,
) -> Result<(), ResponseInvariantError> {
    match operation {
        "snapshot_page" => {
            let visual_viewport = result.get("visual_viewport") ...;
            project_root_snapshot(result, response.detail, SnapshotNovelty::Novel,
                                  visual_viewport.as_ref())
        }
        "inspect_page" => project_root_page_state(result, response.detail),
        _ => Ok(()),
    }
}
```

`project_response` delegates its `snapshot_page`/`inspect_page` arms to this
helper (image-clearing stays where it is). `project_batch_step` gains the
step `operation` parameter and, after `project_operation` + removing the
`observation` key, calls `project_tool_root(operation, &mut value, response)`
when the projected value is an object.

**Implementation Notes**:
- `step.operation` is the batchable operation enum; map/serialize it to the
  same stable string the standalone tools use. If it is already a string-typed
  enum, pass `operation.as_ref()`/serialized form.
- Batch `snapshot_page` steps embed the snapshot at the step-result root (same
  shape a standalone `snapshot_page` returns before projection), so
  `project_root_snapshot` applies unchanged.

**Acceptance Criteria**:
- [x] Batch containing a `snapshot_page` step at concise detail projects the
      step result to the bounded `targets` ranking (no raw `nodes` array);
      assert a large-snapshot step stays under a bound (e.g. < 32 KB) where the
      unprojected tree would exceed it.
- [x] Batch `inspect_page` step at concise detail yields concise page state,
      matching the standalone concise `inspect_page` shape.
- [x] `detail: full` batch step still returns the complete snapshot (parity
      with standalone full).
- [x] Non-snapshot/inspect steps are byte-identical to today (regression).

## Testing
- Interface test in response.rs tests: a two-step batch (some interaction +
  `snapshot_page`) at concise detail, asserting the step result carries
  `targets`/no `nodes` and is bounded; a full-detail variant asserting parity
  with standalone full. Reuse the existing large-snapshot fixture used by the
  standalone concise-snapshot tests.
- Regression: existing batch projection tests (step observation removal, failed
  step propagation) stay green.

## Risks
- The step `operation` string must match the tool name `project_response`
  keys on. If the batch operation enum serializes differently, normalize at the
  call site; a mismatch would silently skip projection (caught by the concise
  acceptance test).

Origin: `.work/backlog/idea-batch-step-snapshot-projection-bypass.md`.

## Implementation notes

- Added `project_tool_root`, keyed by `BrowserOperationKind::stable_name()`, and
  used it from both standalone response projection and batch step projection.
  This keeps the canonical result acquisition and detail-tiered presentation
  path shared without changing the wire or domain types.
- Added deterministic response tests covering a large concise snapshot,
  concise inspect-page parity, full snapshot parity, and the existing batch
  observation-removal/failure behavior.
