# Current SQL Schema Boundary

Bootstrap one declarative current SQLite schema and replace incompatible disposable cache before use.

## Rationale

Krometrail has no supported integration that requires historical store upgrades, and retained browser evidence is disposable cache. One current schema is smaller and easier to audit while an exact version check prevents older or newer data from being interpreted partially. An incompatible cache is removed at its ownership boundary and initialized directly to the current shape so it cannot block agent startup. Configuration, managed browser profiles, diagnostics, and unknown data-root members remain outside that reset.

## Examples

- `crates/krometrail-store/src/index/schema.rs` declares the complete current STRICT schema.
- `initialize_or_validate` initializes an empty database transactionally, opens the exact current version without writes, and classifies every other shape as incompatible without mutating it.
- `SqliteIndex::open` closes an incompatible index, clears the allowlisted recording-cache members, and initializes the current schema before continuing.

## When to Use

Use for the Krometrail metadata store and other retained formats whose supported consumer is the current agent runtime.

## When Not to Use

Do not use this pattern to bypass a concrete supported data-upgrade requirement. Identify that consumer and design the upgrade boundary explicitly first.

## Common Violations

- Adding historical migration chains without a supported consumer.
- Treating an unversioned non-empty database as fresh.
- Trying to interpret or migrate an incompatible schema before clearing the disposable cache.
- Removing an entire shared data root instead of the allowlisted recording-cache members.
- Scattering current schema creation through runtime code.
- Omitting indexes, triggers, foreign keys, or strict constraints from shape coverage.
