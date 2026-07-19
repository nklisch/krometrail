# Current SQL Schema Boundary

Bootstrap one declarative current SQLite schema and reject incompatible retained formats before mutation.

## Rationale

Krometrail has no supported integration that requires historical store upgrades. One current schema is smaller and easier to audit while an exact version check prevents older or newer data from being interpreted partially.

## Examples

- `crates/krometrail-store/src/index/schema.rs` declares the complete current STRICT schema.
- `initialize_or_validate` initializes an empty database transactionally, opens the exact current version without writes, and rejects every other shape with a recovery action.

## When to Use

Use for the Krometrail metadata store and other retained formats whose supported consumer is the current agent runtime.

## When Not to Use

Do not use this pattern to bypass a concrete supported data-upgrade requirement. Identify that consumer and design the upgrade boundary explicitly first.

## Common Violations

- Adding historical migration chains without a supported consumer.
- Treating an unversioned non-empty database as fresh.
- Mutating an incompatible schema before rejecting it.
- Scattering current schema creation through runtime code.
- Omitting indexes, triggers, foreign keys, or strict constraints from shape coverage.
