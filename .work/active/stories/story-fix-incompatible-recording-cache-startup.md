---
id: story-fix-incompatible-recording-cache-startup
kind: story
stage: done
tags: [bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Clear incompatible recording cache during startup

## Symptom

After upgrading from Krometrail 1.2.0, Codex reports no Krometrail tools because the MCP process exits with `persistence_failed`: metadata schema version 6 is incompatible with the schema-7 build.

## Root cause

`SqliteIndex::open` treats every non-current metadata schema as durable user data and aborts startup. Krometrail's recording index, segments, artifacts, and deletion staging are disposable cache data, so a release schema bump incorrectly makes retained evidence a startup dependency.

## Fix approach

Classify incompatible schemas before mutation, close the old connection, remove only the known recording-cache members, initialize the one current schema, and continue startup. Preserve every other data-root member, including managed browser profiles, diagnostics, and configuration-like files. Update the current contract and agent setup guidance so future schema changes retain this behavior.

## Regression test

`crates/krometrail-store/tests/sqlite_schema.rs` creates a schema-6 cache with segments, artifacts, and deletion staging, verifies startup replaces it with schema 7, and verifies unrelated profile/configuration data remains untouched. Before the fix it fails with the reported `persistence_failed` error.

## Implementation notes

- **Execution capability:** direct host-agent implementation; the verified regression was narrow to the store startup boundary and did not warrant independent or delegated work.
- **Files changed:** the SQLite schema classifier and open/reset boundary, store and real-MCP regression tests, the current-schema pattern, AGENTS contract, foundation storage contracts, troubleshooting guidance, generated public documentation, and the Krometrail setup skill.
- **Regression evidence:** the new store test failed before implementation with the exact schema-6 `persistence_failed` error and passes afterward. A binary smoke test now starts the real MCP process against schema 6, observes clean protocol shutdown, verifies schema 7, verifies stale segments are gone, and verifies the managed profile remains.
- **Four-step confirmation:** the focused regression passes; the full workspace test suite passes with loopback permission; the original MCP-startup reproduction passes through `tests/rust-runtime-smoke.rs`; and the live schema-6 cache was cleared while managed profiles and diagnostics remained. Formatting, workspace check, clippy with warnings denied, and the documentation build are green.
- **Adjacent issues parked:** none.

## Review (2026-07-19)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: Bounded inline standalone-story review; no independent or cross-model reviewer ran. Correctness review confirmed that classification occurs before mutation, the old connection closes before deletion, reset is allowlisted to recording-cache members, and initialization retries once. Tests cover older, newer, and unversioned incompatible schemas plus the real MCP startup path. File-path handling remains fixed to owner-provided cache paths and emits source-safe failures. Public storage contracts, project patterns, troubleshooting, generated docs, and shipped skill guidance agree with the implementation. Feature/epic integration lenses were skipped because this is a standalone targeted repair.
