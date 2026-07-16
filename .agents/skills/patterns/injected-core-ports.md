# Core Ports with Composition-Root Assembly

Keep domain services dependent on inward-facing ports and assemble concrete infrastructure only at the root composition boundary.

## Rationale

Dependency direction stays inward, infrastructure remains replaceable, and tests can inject clocks, IDs, browser transports, stores, and analysis services.

## Examples

- `crates/krometrail-core/src/ports/recording.rs:9` — `RecordingSink` defines persistence behavior without a database/filesystem dependency.
- `src/app.rs:40` — `RuntimeDependencies` collects inward-facing trait objects.
- `src/artifacts/service.rs:60` — temporal artifact generation receives frame/store/ID ports.
- `src/app.rs:180` — the root wires `ProductionBrowserConnector`, launcher, transport, capture, and storage.

## When to Use

Use at storage, browser transport, time/randomness, retention, and visual-analysis boundaries.

## When Not to Use

Do not abstract pure local helpers or internal concrete dependencies that are part of a module's responsibility.

## Common Violations

- Importing CDP or SQLite into core.
- Constructing adapters inside handlers.
- Bypassing an injected port.
- Creating one oversized interface for unrelated concerns.
