# Registry-Declared Variant Surfaces

Declare growing variants once, then derive stable identities, metadata, schemas, validation, and downstream registration from that declaration.

## Rationale

Capabilities, browser operations, browser events, evidence operations, and MCP routes must not drift across parallel lists. One registry is authoritative and consumers project from it.

## Examples

- `crates/krometrail-core/src/capabilities/mod.rs:51` — `CAPABILITY_REGISTRY` is emitted with IDs, defaults, dependencies, and recording subsystems.
- `crates/krometrail-core/src/browser/operation.rs:119` — `BrowserOperationKind::ALL`, stable names, and reverse lookup derive from the operation declaration/registry.
- `crates/krometrail-core/src/browser/events.rs:1618` — event kinds resolve their definition through `BROWSER_EVENT_REGISTRY`.
- `crates/krometrail-mcp/src/registry.rs:177` — MCP progressive routes iterate the core registry and generated input schema.

## When to Use

Use for capability, operation, event, artifact, or plugin sets consumed by multiple runtime or public-contract surfaces.

## When Not to Use

Do not introduce a registry for a small closed detail with one consumer or variants with unrelated ownership/lifecycle models.

## Common Violations

- Maintaining a second identity list.
- Hard-coding route names in handlers.
- Deriving metadata independently.
- Adding a variant without completeness/order tests.
