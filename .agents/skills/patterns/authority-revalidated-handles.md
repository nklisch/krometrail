# Authority-Revalidated Opaque Handles

Revalidate an opaque handle's scope, generation, backing identity, and current availability every time it is dereferenced.

## Rationale

Krometrail handles cross mutable browser, process, retention, and resource boundaries. Possession proves only that an identity was once issued; it does not prove the document, frames, resource, or session remains authoritative.

## Examples

### Node references revalidate document authority

**File**: `crates/krometrail-cdp/src/control/snapshot.rs:558`

```rust
let (document, backend) = self.active_reference_backend(bound, reference)?;
let current = document_fingerprint(transport, &scope, bound.target_id).await?;
if current != *document {
    return Err(stale(bound.target_id, "document changed after the snapshot"));
}
```

### Range handles revalidate retained frames

**File**: `src/range_handles.rs:104`

```rust
let metadata = read_available_metadata(self.frames.as_ref(), &range).await?;
validate_available_metadata(&range, &metadata)?;
```

### Resource reads verify requested identity

**File**: `crates/krometrail-mcp/src/resources.rs:597`

```rust
if read.handle.artifact_id != expected_id
    || read.handle.scope != parsed.scope
    || expected_uri.canonical_uri() != uri
{
    return Err(rmcp::ErrorData::internal_error("resource handle identity mismatch", None));
}
```

## When to Use

- Process-local handles, resource URIs, snapshot references, and retained-evidence identities.
- State that can disappear, be evicted, reconnect, navigate, or change generation.
- Cross-layer adapters whose returned identity must match the requested authority.

## When NOT to Use

- Self-contained immutable values requiring no external lookup.
- Keys used entirely under one lock and authority lifetime.
- Validation that merely duplicates pure-domain invariants already proven.

## Common Violations

- Treating possession as authorization or availability.
- Validating only when the handle is issued.
- Rebinding a stale reference to a convenient current object.
- Returning partially matching scope or identities.
- Converting authority failures into empty successes.
