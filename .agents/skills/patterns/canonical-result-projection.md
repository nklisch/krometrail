# Canonical Result Projection

Acquire the canonical domain result first, then apply an additive presentation projection without changing authoritative outcomes, warnings, anchors, or resource identities.

## Rationale

Routine agent responses must stay concise without creating cheaper parallel acquisition paths or weakening the current domain contract. Centralized projection lets operations, status, and temporal evidence share one omission-first default while preserving deliberate expanded and full detail.

## Examples

### Browser operations project after canonical mapping

**File**: `crates/krometrail-mcp/src/response.rs`

```rust
let mut projection = project_operation(result, response)?;
let target_id = projection_target_id(&projection);
add_capture_warnings(&mut projection, capture_statuses, target_id);
project_response(tool, &mut projection, response)?;
```

### Concise status derives from complete status

**File**: `crates/krometrail-mcp/src/response.rs`

```rust
match response.detail {
    ResponseDetail::Full => map_lifecycle_result(tool, status),
    ResponseDetail::Concise | ResponseDetail::Expanded => {
        let capture = status.capture.iter().map(|capture| ConciseCaptureStatus {
```

### Inline images are orthogonal to structured detail

**File**: `crates/krometrail-mcp/src/response.rs`

```rust
let mut projection = match result {
    ProgressiveEvidenceResult::FetchSourceFrames(batch) => {
        project_source_frame_batch(*batch, response.inline_images)?
    }
    // Other canonical result variants map here before presentation.
};
project_response(tool, &mut projection, response)?;
Ok(mapped(tool, projection, format!("{tool} succeeded")))
```

## When to Use

- Agent responses whose canonical result can be expensive in context size.
- Surfaces offering concise, expanded, or full structured detail and optional inline pixels.
- Projections that must retain current errors, warnings, interaction anchors, and drill-down resources.

## When NOT to Use

- Domain acquisition, persistence, authorization, or retention decisions.
- A projection that would invent a second status, outcome, or evidence authority.
- Values already appropriately sized with no presentation variants.

## Common Violations

- Skipping acquisition because a field will be omitted.
- Letting concise mode change success, degradation, warnings, or retry meaning.
- Removing canonical resources together with inline bytes.
- Implementing projection independently in handlers.
- Making full presentation the implicit default instead of requiring deliberate expansion.
