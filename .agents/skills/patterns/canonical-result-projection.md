# Canonical Result Projection

Acquire the canonical domain result first, then apply an additive presentation projection without changing authoritative outcomes, warnings, anchors, or resource identities.

## Rationale

Routine agent responses must stay compact without creating cheaper parallel acquisition paths or weakening the stable domain contract. Centralized projection lets operations, status, and temporal evidence share economical defaults while preserving explicit full-detail expansion.

## Examples

### Browser operations project after canonical mapping

**File**: `crates/krometrail-mcp/src/response.rs:435`

```rust
let mut projection = project_operation(result, preference)?;
let target_id = projection_target_id(&projection);
add_capture_warnings(&mut projection, capture_statuses, target_id);
apply_response_projection(tool, &mut projection, preference)?;
```

### Concise status derives from complete status

**File**: `crates/krometrail-mcp/src/response.rs:503`

```rust
match detail {
    BrowserStatusDetail::Full => map_lifecycle_result(tool, status),
    BrowserStatusDetail::Concise => {
        let capture = status.capture.iter().map(|capture| ConciseCaptureStatus {
```

### Progressive evidence maps before inline omission

**File**: `crates/krometrail-mcp/src/response.rs:1595`

```rust
let mut mapped = map_progressive_result(tool, result)?;
if preference.inline_images == InlineImageDetail::Omit {
    mapped.images.clear();
    mapped.response.images.clear();
}
```

## When to Use

- Agent responses whose canonical result can be expensive in context size.
- Surfaces offering compact, full, omitted, or inline-resource presentation choices.
- Projections that must retain stable errors, warnings, interaction anchors, and drill-down resources.

## When NOT to Use

- Domain acquisition, persistence, authorization, or retention decisions.
- A projection that would invent a second status, outcome, or evidence authority.
- Values already appropriately sized with no presentation variants.

## Common Violations

- Skipping acquisition because a field will be omitted.
- Letting compact mode change success, degradation, warnings, or retry meaning.
- Removing canonical resources together with inline bytes.
- Implementing projection independently in handlers.
- Making the expensive legacy presentation the implicit default.
