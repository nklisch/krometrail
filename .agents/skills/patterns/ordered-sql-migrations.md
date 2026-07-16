# Ordered SQL Migration Registry

Store each SQLite schema revision as an immutable SQL module and apply contiguous revisions transactionally through one migration registry.

## Rationale

Retained recording data upgrades from known prior revisions with centralized ordering, version checks, and rollback behavior.

## Examples

- `crates/krometrail-store/src/index/migrations.rs:12` — `MIGRATIONS` and `LATEST_SCHEMA_VERSION` define contiguous order.
- `crates/krometrail-store/src/index/schema_v1.rs:1` — the initial immutable STRICT schema.
- `crates/krometrail-store/src/index/schema_v2.rs:1` — an isolated additive/backfill revision.
- `crates/krometrail-store/src/index/schema_v3.rs:1` — interaction persistence introduced as its own revision.

## When to Use

Use for persisted SQLite formats that must upgrade retained data from known revisions.

## When Not to Use

Do not add migrations to disposable in-memory schemas or formats with no retained upgrade contract.

## Common Violations

- Editing historical migration SQL.
- Skipping version numbers.
- Changing the latest version without a migration.
- Applying steps outside the migration transaction.
- Scattering schema mutation through runtime code.
