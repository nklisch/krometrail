---
id: epic-agent-browser-reliability-agent-contracts
kind: feature
stage: done
tags: [agent-ux, browser, storage]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics, epic-agent-browser-reliability-capture-outcomes, epic-agent-browser-reliability-managed-session-lifecycle, epic-agent-browser-reliability-interaction-semantics, epic-agent-browser-reliability-viewport-emulation]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Precise agent contracts and guidance

## Brief

Resolve GitHub issues #6 and #12 after the runtime contracts they describe are complete. Publish MCP input schemas whose nested locator, target, modifier, fill, viewport, temporal range, and selection unions survive Codex declaration projection instead of becoming `unknown`, while keeping canonical Rust-generated schemas authoritative. Invalid requests identify the first mismatched field path without echoing sensitive values.

Update the Krometrail skill with valid CSS-selector and snapshot-reference examples, safe defaults, the economical interaction-evidence hierarchy, capture-health prerequisites, compositor/partial-frame recovery, and targeted diagnostic-log collection by correlation identifier. The guidance must distinguish automatically returned post-operation screenshots, `observe_live`, and persisted source frames.

Ship a second, independently triggered plugin skill for reporting Krometrail defects to GitHub with authenticated `gh`. It searches for duplicates, collects version/platform/operation/correlation context, extracts only bounded sanitized diagnostics, requires expected-versus-actual reproduction detail, and confirms the external write. It never attaches a whole log or discloses browser content, form values, secrets, screenshots, raw CDP traffic, or unredacted URLs.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: terminal consumer of every runtime feature so generated declarations and prose match shipped behavior.

## Simplification opportunity
- Normalize generated schemas at the MCP boundary and derive examples from stable request shapes rather than maintaining parallel handwritten type inventories.

## Foundation references
- `docs/SPEC.md` — MCP schemas, errors, and browser-control surface
- `docs/ARCHITECTURE.md` — registry-derived tools and generated contracts
- `docs/VISUAL-EVIDENCE.md` — evidence hierarchy and provenance

## Design decisions

- **Schema projection**: recursively inline local `$ref` values from `$defs`, preserving generated constraints. Reject cycles or unresolved references during router construction.
- **Validation detail**: deserialize with path tracking and expose only the first structural field path plus stable error code, never the rejected value.
- **Skill split**: retain browser/evidence guidance in `krometrail`; add `report-krometrail-issue` because reporting has a distinct trigger, privacy workflow, and external-write confirmation.
- **Destination**: use the plugin's canonical public GitHub repository rather than whichever repository is current.

## Architectural choice

Normalize schemas only at MCP publication. Canonical types remain generated in core, avoiding a handwritten inventory. Keep both skills concise and standalone. A combined skill was rejected because normal browser use should not load issue-reporting instructions; handwritten flat schemas were rejected because they drift.

## Implementation units

### Unit 1: declaration-friendly schemas

**File**: `crates/krometrail-mcp/src/schema.rs`

```rust
fn dereference_local_schema(root: Value) -> Result<Value>;
fn resolve_ref(root: &Value, reference: &str, stack: &mut Vec<String>) -> Result<Value>;
```

Resolve RFC 6901 local references rooted at `#/$defs/`, merge reference-site annotations without weakening the target, remove unused `$defs`, and fail closed on cycles/missing definitions. Apply after batch filtering to every schema.

**Acceptance criteria**:
- [x] Published schemas contain no `$ref`/`$defs` and retain nested object/union constraints.
- [x] Batch filtering still derives from the complete canonical union.
- [x] Cycles or missing definitions produce a stable startup error.

### Unit 2: precise invalid-argument paths

**Files**: `crates/krometrail-mcp/src/registry.rs`, `crates/krometrail-mcp/Cargo.toml`

```rust
fn decode_arguments<T: DeserializeOwned>(arguments: JsonObject) -> Result<T>;
fn invalid_arguments(path: &str) -> KrometrailError;
```

Use `serde_path_to_error` over JSON deserialization, normalize its path notation, and return `invalid_input` naming the first mismatched field. Exclude serde text that could echo caller values.

**Acceptance criteria**:
- [x] Nested failures name paths such as `locator.reference` without echoing values.
- [x] Existing valid requests remain valid.

### Unit 3: economical browser-evidence guidance

**Files**: `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`, `plugin/skills/krometrail/references/setup.md`, `plugin/skills/krometrail/agents/openai.yaml`

Lead with cheapest sufficient evidence: trust automatic post-operation evidence for immediate confirmation, use `observe_live` for explicit current state, and use retained source/derived evidence only for history or ambiguity. Document defaults, reference validity, compositor/capture warnings, viewport control, and correlation-bounded diagnostic lookup.

**Acceptance criteria**:
- [x] Examples use valid selectors/references and do not require a redundant screenshot after every action.
- [x] Live post-action, current observation, and retained source frames are distinct.
- [x] Diagnostic instructions collect only a targeted excerpt.

### Unit 4: GitHub issue-reporting skill

**Files**: `plugin/skills/report-krometrail-issue/SKILL.md`, `plugin/skills/report-krometrail-issue/agents/openai.yaml`, plugin validation/package tests if required

Workflow: verify `gh auth status`; select canonical repo; collect version, OS/architecture, tool/operation, stable error code, correlation ID/path, expected/actual/reproduction; search all issue states and inspect candidates; extract a narrow correlation-centered excerpt and redact defensively; draft; confirm; run `gh issue create`; return URL.

**Acceptance criteria**:
- [x] The new skill has a distinct trigger and stays under 300 lines.
- [x] It forbids full logs and sensitive browser/user content.
- [x] Duplicate search and confirmation precede issue creation.
- [x] Skill validation and plugin packaging discover both skills.

## Implementation order

1. Schema dereferencing and path-aware errors.
2. Runtime-aware Krometrail skill update.
3. Independent GitHub reporting skill and validation.

## Simplification

- Preserve one Rust request/schema authority with a mechanical MCP projection.
- Split the rare external write from common browser use to reduce context and accidental writes.

## Testing

- Golden schemas recursively assert no references and verify nested locator, modifier, temporal-selection, and viewport shapes.
- Argument tests verify safe first-path reporting and no value leakage.
- Validate both skill folders and package discovery; forward-test reporting with a fresh agent in draft-only mode.

## Risks

- Recursive schemas can expand indefinitely; reject cycles instead of inventing an incompatible depth cap.
- GitHub search can miss semantic duplicates; inspect candidates and state why a new report differs.

## Implementation notes

- Execution capability: strongest implementation owner; this changes stable MCP declarations,
  invalid-input disclosure, plugin guidance, and a confirmation-gated external-write workflow.
- Review weight: standard, inherited from the parent epic autopilot caller.
- Files changed: `crates/krometrail-mcp/src/schema.rs`,
  `crates/krometrail-mcp/src/registry.rs`, MCP dependency manifests/lockfile,
  `plugin/skills/krometrail/**`, `plugin/skills/report-krometrail-issue/**`, and plugin static/install
  discovery tests.
- Tests added/updated: reference-free schema publication, annotation/constraint preservation,
  missing/cyclic reference rejection, safe nested argument paths without value leakage, two-skill
  package discovery, and a draft-before-submit ordering check.
- Simplification: one mechanical projection inlines canonical generated schemas at publication; one
  path-aware decoder covers lifecycle, browser, progressive-evidence, and temporal inputs; rare
  GitHub reporting remains separate from common browser operation guidance.
- Discrepancies from design: none. Existing runtime diagnostics expose the designed correlation ID
  and private log path, so the skills use those fields directly.
- Adjacent issues parked: none.

## Integrated verification

- `cargo test -p krometrail-mcp schema::tests --locked` — 5 passed.
- `cargo test -p krometrail-mcp registry::tests::invalid_arguments_name_first_nested_path_without_echoing_values --locked` — passed.
- `cargo test -p krometrail-mcp registry::tests::route_registry_and_schema_validation_fail_closed --locked` — passed.
- Skill Creator `quick_validate.py` — both skills valid using an isolated PyYAML runtime.
- `bash tests/plugin-static.sh` — passed and discovers both skills.
- Draft-only forward check — authenticated `gh`, canonical-repository read/search, and static workflow
  ordering passed; no issue or comment was created.
- Full workspace formatting/gates and native plugin install smoke remain with the root integration
  owner because concurrent runtime features share the working tree and release bundle.

## Review record

- Effective weight: standard; pass: 1; verdict: approve after fixes.
- Findings fixed: viewport schema exposes runtime constraints; duplicate search and every outbound issue field use a fixed privacy whitelist; diagnostic extraction is match- and context-bounded; public and generated guidance cover viewport evidence.
- Verification: reference-free schema publication, safe nested paths, both skill validators, plugin static/package contracts, docs build, full workspace, and strict clippy passed.
