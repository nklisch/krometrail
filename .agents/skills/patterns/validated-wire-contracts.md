# Constructor-Backed Wire Contracts

Deserialize a wire helper, validate through the domain constructor, and delegate JSON Schema to the same accepted wire shape.

## Rationale

External input must not create invalid domain state, while generated schemas must describe the actual accepted representation rather than a hand-maintained approximation.

## Examples

- `crates/krometrail-core/src/validation.rs:28` — `deserialize_validated` decodes a wire value and maps domain errors into serde errors.
- `crates/krometrail-core/src/ports/browser.rs:128` — `AttachBrowserWire` carries the external shape and `delegate_json_schema!` keeps the public schema aligned.
- `crates/krometrail-core/src/timeline/context.rs:391` — browser-event detail requests deserialize through `into_request` validation.
- `crates/krometrail-core/src/artifacts.rs:439` — artifact-generation requests construct through their invariant-enforcing constructor.

## When to Use

Use for public requests, persisted records, or nested values whose invariants are stronger than their serialized primitives.

## When Not to Use

Skip the pattern for trusted internal values or simple immutable data with no validation.

## Common Violations

- Deriving `Deserialize` directly on invariant-bearing types.
- Validating only in MCP handlers.
- Duplicating schema constraints separately.
- Accidentally accepting unknown fields.
