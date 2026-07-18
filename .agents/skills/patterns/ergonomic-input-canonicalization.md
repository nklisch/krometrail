# Ergonomic Input to Canonical Authority

Resolve ergonomic presets, semantic queries, and opaque shortcuts into existing explicit domain authorities before downstream execution.

## Rationale

Agent ergonomics should reduce orchestration cost without creating parallel identity, geometry, or temporal models. Materializing convenience inputs at a boundary preserves one execution authority and keeps original intent only as presentation or provenance.

## Examples

### Semantic queries return exact references

**File**: `crates/krometrail-cdp/src/control/snapshot.rs:453`

```rust
let matches = snapshot.nodes.iter().filter_map(|node| {
    let reference = node.reference?;
    semantic_query_matches(&request.query, node, metadata).then(|| SemanticMatch {
        reference,
        role: node.role.clone(),
        name: node.name.clone(),
    })
}).collect();
```

### Viewport presets materialize into metrics

**File**: `crates/krometrail-core/src/browser/viewport.rs:247`

```rust
Self::Preset { preset } => ViewportMaterialization {
    intent: preset.intent(),
    preset: Some(preset),
    metrics: Some(preset.materialize()),
    user_agent_emulated: false,
},
```

### Range handles restore the canonical range request

**File**: `crates/krometrail-mcp/src/registry.rs:659`

```rust
let range = handles.resolve_available(handle_argument.range_handle).await?;
arguments.insert("range".into(), serializable(range)?);
```

## When to Use

- Adding agent-friendly aliases, presets, locators, defaults, or handles.
- A mature canonical domain value already governs execution or persistence.
- Convenience intent is useful provenance but must not become authority.

## When NOT to Use

- The convenience form represents genuinely new domain semantics.
- Resolution would be ambiguous or lossy without an explicit outcome.
- The input is already the canonical authority.

## Common Violations

- Passing preset names into infrastructure instead of materialized metrics.
- Letting semantic text directly authorize mutation.
- Teaching storage services about process-local handles.
- Maintaining parallel lifecycle state for ergonomic and canonical forms.
- Silently selecting an ambiguous or truncated semantic match.
