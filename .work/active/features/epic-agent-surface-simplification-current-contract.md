---
id: epic-agent-surface-simplification-current-contract
kind: feature
stage: done
tags: [infra, storage]
parent: epic-agent-surface-simplification
depends_on: []
release_binding: 1.2.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# One current Krometrail contract

## Brief

Remove runtime machinery that exists only to preserve unsupported historical Krometrail releases or hypothetical crate consumers. Keep one current store schema that opens current-format data and rejects older incompatible data before mutation with a clear recovery action. Remove old installer cutoffs, compatibility aliases, default port implementations retained for source compatibility, and contradictory policy/test prose.

This feature does not remove Chrome/CDP compatibility probing, deterministic stable names, evidence algorithm versions, visual-epoch compatibility, or integrity/version checks for the current format. Those are current correctness and provenance mechanisms rather than integration shims.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: contract foundation; response simplification depends on this direction

## Simplification opportunity

Collapse the ordered historical SQL migration chain into current-schema bootstrap plus exact current-version validation; delete unsupported installer upgrade branches/tests, type aliases, const constructors, and trait defaults whose comments identify source compatibility as their sole purpose.

## Foundation references

- `.agents/AGENTS.md` — Current Contract Discipline
- `docs/SPEC.md` — current executable and retained-data contracts
- `docs/ARCHITECTURE.md` — recording store and generated MCP boundary

## Design decisions

- **Store version**: retain schema version `6` as the identity of the current on-disk shape. A new empty database is initialized directly to that shape; version 6 opens; every other version fails before schema mutation with recovery to remove/archive the data directory.
- **Current data**: do not rebuild or rewrite a current v6 database merely to remove migration code. Current recordings remain usable without a migration step.
- **Port obligations**: progressive frame reads are required `FrameSource` methods. Test doubles either implement their real supported behavior or return an explicit unsupported error themselves; the trait no longer supplies compatibility defaults.
- **Spike vocabulary**: the only transport evidence/decision Rust types are `TransportEvidenceV2` and `TransportDecisionV2`; rename live call sites directly and keep the serialized `schema_version` validation.
- **Installer boundary**: validate requested release syntax and binary/checksum identity, but remove the TypeScript-era cutoff and comparison helpers. GitHub release availability is the release-selection authority.

## Architectural choice

Three store approaches were considered. Keeping migrations contradicts the current-contract policy. Recreating every existing database on each release would discard current evidence unnecessarily. The chosen approach uses one declarative current schema plus a small initialize-or-validate boundary: empty databases are created transactionally, exact-current databases open untouched, and incompatible versions fail clearly. This is the least code while preserving current data and preventing accidental partial interpretation.

The riskiest unit is producing one current schema without losing triggers, indexes, strict-table constraints, or foreign keys accumulated across six revisions. The implementation derives and reviews the final SQL against a migrated-v6 reference in tests, then deletes the historical inputs.

## Implementation Units

### Unit 1: One current SQLite schema

**Files**: `crates/krometrail-store/src/index/schema.rs`, `crates/krometrail-store/src/index/mod.rs`; delete `migrations.rs` and `schema_v1.rs` through `schema_v6.rs`
**Story**: `epic-agent-surface-simplification-current-contract-current-store-schema`

```rust
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 6;
pub(crate) const CURRENT_SCHEMA_SQL: &str = r#"/* complete current schema */"#;

pub(crate) fn initialize_or_validate(
    connection: &mut rusqlite::Connection,
) -> krometrail_core::Result<()>;
```

**Implementation notes**:
- Read `PRAGMA user_version` before mutation. `0` is initializable only when no Krometrail user tables exist; an unversioned non-empty database is incompatible.
- Initialize current SQL and set `user_version=6` inside one exclusive transaction.
- Version 6 returns without schema writes. Any other value returns a persistence error with a clear archive/remove-and-restart recovery action; no old DDL executes.
- Consolidate maintenance code that names “legacy” rows only if it still repairs a possible current-v6 inconsistency; delete it when its input cannot exist in current schema.

**Acceptance criteria**:
- [ ] Empty storage initializes directly to the complete strict v6 schema in one transaction.
- [ ] A populated current v6 fixture opens byte-for-byte without data/schema mutation.
- [ ] Versions 0-with-tables, 1–5, and greater than 6 fail before mutation with bounded recovery guidance.
- [ ] A schema-shape comparison covers tables, columns, indexes, triggers, foreign keys, and user version before historical files are deleted.

### Unit 2: Required current core contracts

**Files**: `crates/krometrail-core/src/ports/frames.rs`, all `FrameSource` implementations; `crates/krometrail-core/src/browser/control.rs` and `ListPagesRequest` call sites
**Story**: `epic-agent-surface-simplification-current-contract-remove-runtime-shims`

```rust
pub trait FrameSource: Send + Sync {
    fn list_source_frames(&self, request: SourceFramesRequest)
        -> PortFuture<'_, Result<SourceFrameList>>;
    fn fetch_source_frames(&self, request: SourceFramesRequest)
        -> PortFuture<'_, Result<SourceFrameBatch>>;
    fn read_source_frame(&self, request: RetrieveSourceFrameRequest)
        -> PortFuture<'_, Result<SourceFrameRead>>;
    // existing required methods
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, JsonSchema)]
pub struct ListPagesRequest {}
```

**Implementation notes**:
- Delete `progressive_read_unsupported` from the trait module after updating every implementation explicitly.
- Delete the value-namespace `ListPagesRequest` constant. Construct the unit-like object as `ListPagesRequest {}` at every call site.
- Do not remove current input validation or reference/availability checks.

**Acceptance criteria**:
- [ ] All production adapters and test doubles state their current progressive-read behavior explicitly.
- [ ] No compatibility comment/default/const remains, and operation schemas still publish an object request.

### Unit 3: Current transport and installer vocabulary

**Files**: `crates/krometrail-cdp/src/spike/evidence.rs`, `crates/krometrail-cdp/src/spike/mod.rs`, spike/harness/tests; `scripts/install.sh`, `tests/installer-fixtures.sh`
**Story**: `epic-agent-surface-simplification-current-contract-remove-runtime-shims`

```rust
pub struct TransportEvidenceV2 { /* current fields */ }
pub struct TransportDecisionV2 { /* current fields */ }

fn validate_release_version(candidate: &str) -> bool;
```

**Implementation notes**:
- Rename every `V1` alias use to the real `V2` type and delete aliases/comments; serialized `schema_version` remains validated current provenance.
- Delete `LEGACY_RELEASE_CUTOFF`, decimal/version comparison, `is_legacy_release_version`, `reject_legacy_release`, and legacy rejection fixture branches.
- Keep simple release syntax validation, checksum verification, executable identity/version checks, exact asset selection, and atomic installation.

**Acceptance criteria**:
- [ ] No Rust V1 transport alias or live call site remains.
- [ ] Installer accepts any syntactically valid explicitly requested published version and still refuses malformed versions, checksum/identity mismatches, and unsafe replacement.
- [ ] Installer fixture coverage is shorter and contains no legacy-runtime cutoff scenario.

### Unit 4: Contract documentation and cruft assertion

**Files**: `.agents/AGENTS.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `.agents/skills/patterns/*.md`, generated docs if affected
**Story**: `epic-agent-surface-simplification-current-contract-remove-runtime-shims`

**Implementation notes**:
- Verify the preflight current-contract language matches delivered behavior.
- Update/delete pattern prose that makes unsupported compatibility preservation a requirement; retain current exact release activation and current schema integrity guidance.
- Run a bounded source search for compatibility-only vocabulary and adjudicate each hit by the feature's explicit boundary rather than deleting browser/evidence correctness concepts.

**Acceptance criteria**:
- [ ] Current instructions no longer direct future agents to add unsupported compatibility machinery.
- [ ] Remaining compatibility/stable/version terms each describe current protocol qualification, deterministic evidence, current format identity, or release correctness.

## Implementation Order

1. `epic-agent-surface-simplification-current-contract-current-store-schema`
2. `epic-agent-surface-simplification-current-contract-remove-runtime-shims`

## Simplification

- Delete seven historical store migration/schema modules and migration-only tests in favor of one schema module and four boundary tests.
- Delete three `FrameSource` method defaults, their shared fallback helper/imports, the `ListPagesRequest` value shim, two transport aliases, release-cutoff comparison functions, and legacy installer fixtures.
- Remove comments/tests whose only assertion is preservation of deleted shapes; retain integrity and current behavior tests.

## Testing

- Store schema interface tests protect empty initialization, exact-current open, incompatible refusal without mutation, and full current shape.
- Workspace compilation enumerates every required `FrameSource` implementation; focused progressive tests verify production behavior.
- Existing transport contract tests run under V2 names without duplicate alias tests.
- Installer fixtures retain syntax/checksum/identity/atomic replacement/path coverage while removing historical cutoff cases.
- Run workspace fmt/check/test/clippy after focused crates/scripts.

## Risks

The single schema can accidentally omit a trigger or index even while tables compile; exact schema-shape comparison is required before deleting migration sources. An empty database with user-created tables must not be mistaken for a fresh Krometrail store.

## Review

Approved in the single standard fresh-context review pass. The reviewer independently reconstructed the former v1-to-v6 SQLite schema and found all 40 catalog objects identical to the declarative current schema. Focused schema and installer fixtures passed; no blockers, important findings, or retained compatibility-only paths were found.

## Implementation notes

- Execution capability: high; one owner carried the current retained format and runtime/distribution cleanup together so deleted shims were not recreated at an adjacent boundary.
- Review weight: standard from the delegated caller.
- Files changed: current SQLite schema/bootstrap and index composition; required frame-source and list-pages contracts/call sites; CDP spike V2 vocabulary; installer and fixtures; current-contract foundation/pattern guidance.
- Tests added/removed: four current-schema boundary/shape tests replace historical migration coverage; workspace compilation enforces explicit frame-source obligations; current V2 transport and hermetic installer contracts passed.
- Simplification: deleted seven migration/schema modules, historical migration fixtures, core progressive-read defaults, a request value constant, two transport aliases, historical release comparison/cutoff code, and the ordered-migration project pattern.
- Discrepancies from design: the raw `SqliteIndex` explicitly rejects coherent progressive reads because it lacks the `RecordingStore` mutation/revalidation authority; full workspace compilation was owned by the aggregate epic while a concurrent persistence feature changed the shared shutdown API.
- Adjacent issues parked: none.
- Integrated verification: formatting, core all-target check, all 38 store library tests, all 24 CDP spike transport-contract tests, installer fixtures, and staged/working-tree diff checks passed.
