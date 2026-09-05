---
id: epic-a-grade-reliability-release-version-ownership
kind: feature
stage: done
tags: [infra, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Keep independent crate and all plugin versions correct through release

## Outcome and priority

Product 1.6.2 and temporal-vision 0.1.1 cause the lock verifier to throw "Cargo.lock workspace package temporal-vision did not start at 1.6.2". Separately, the new Antigravity manifest is outside the product version projection inventory. Existing distribution fixtures passed despite the real mixed-version workspace failure.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Release verifier failure reproduced; omitted Antigravity version projection code-traced.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized for the bounded checkpoint/design below after the user asked to continue execution. No release or paid model-effectiveness qualification is authorized.

## Evidence

- scripts/bump-version.ts:132,254 — every member enters product-version validation
- scripts/bump-version.ts:152 — derivedVersionPaths omits plugin/plugin.json
- plugin/plugin.json — independently omitted Antigravity projection
- docs/RELEASING.md — temporal-vision release ownership

## Acceptance criteria

- [x] A hermetic fixture mirroring the current mixed-version workspace successfully bumps the product while leaving temporal-vision and unrelated lock packages unchanged.
- [x] Every shipped plugin/catalog version, including plugin/plugin.json and plugin/version, equals the resulting Cargo product version; no projection silently drifts.
- [x] One explicit version-ownership/projection inventory drives updates and validation; fixtures detect a newly shipped unregistered version projection.
- [x] Dry-run, verifier failure, rollback, and independent-crate release boundaries are tested without network, tags, pushes, or mutation of standalone installations.
- [x] Retain exact-version managed activation and Cargo as the sole product release authority.

## Implementation direction and boundaries

Distinguish workspace membership from product-version ownership. Combine the two review findings because one release-ownership contract should prevent both.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Authorized design and implementation boundary — 2026-09-05

The user authorized continued execution. Select one explicit release ownership model: workspace membership alone does not confer product-version ownership; independently versioned manifests and their lock entries remain unchanged. Use one current inventory for every shipped derived version projection, including Antigravity, rather than parallel update/validation lists. Reject inconsistent product-owned inputs before mutations and preserve the existing rollback and exact-version activation contract.

Implement in the release helper and existing hermetic distribution fixtures; add a small cohesive inventory module only if it removes duplication. Fixtures must mirror the real mixed-version workspace, independently verify untouched crate/lock data, cover every shipped projection and an omitted/new projection, dry-run non-mutation, failure/rollback, and independent release boundaries. Demonstrate a meaningful red-to-green regression. Do not actually bump this repository's versions, run a publishing release, create tags, push, or change the independent crate. Preserve command/build fixtures rather than invoking real network release operations. Update essential release documentation only where the corrected contract needs it. Parent review and integration gates precede acceptance.

## Implementation record — 2026-09-05

Implemented by the scoped release-ownership worker on `work/release-ownership`. Files touched: `scripts/release-ownership.ts` (new), `scripts/bump-version.ts`, `tests/distribution-static.sh`, `tests/plugin-static.sh`, `docs/RELEASING.md`, `docs/guide/development.md`, `.agents/skills/patterns/exact-release-managed-activation.md`. No product Rust source, root Cargo version, Cargo.lock, or temporal-vision version was modified; all scenarios ran against throwaway fixture repos under `/tmp`.

### Design decisions

1. **Ownership model is structural, not a second list.** A workspace member is product-version-owned exactly when it explicitly inherits `[workspace.package].version` — `version.workspace = true`, resolved identically in dotted and inline-table form via Bun's TOML parser. Everything else is independent: any version literal (either quote style) and any omitted version, which Cargo defaults to 0.0.0. Alternative rejected: an explicit `independently-versioned` name list — it would duplicate what the manifests already declare and would re-create this same bug the next time a crate goes independent. The existing temporal-vision recoupling guard is retained as the explicit policy backstop.
2. **One inventory module earned its place** (`scripts/release-ownership.ts`): the projection list was previously duplicated between `bump-version.ts` and hardcoded loops in `distribution-static.sh`. The module now exports the registry (`PRODUCT_VERSION_PROJECTIONS`), the shipped-surface scan (`findUnregisteredVersionProjections`), projection preparation/assertion, member-ownership classification, and the shared narrow TOML section reader. The bump helper and the fixtures both consume these exports, so fixture expectations are derived from the same registry the release moves — no parallel update/validation lists.
3. **Unregistered-projection detection is a pre-mutation refusal, not a post-hoc scan.** Before writing anything (including under `--dry-run`), the helper scans `plugin/`, `.claude-plugin/`, `.agents/plugins/` for version-bearing files (files named `version`, JSON with semver-string `"version"` fields at any depth) and refuses the release if any is unregistered. Bounded to the shipped surface so docs/fixtures/lockfile version strings can never block a release. A registered projection that does not carry the current product version is likewise rejected before mutation.
4. **Lock verifier split by ownership.** Product-owned entries (root + inheriting members) must exist exactly once, start at the current version, and be refreshed to the next version. Independent members' lock sections must be byte-identical through the refresh (explicit error, not a generic multiset mismatch). The multiset comparison now normalizes only product-owned entries, so any other lock change — including a drift of the independent crate or an unrelated dependency — fails and rolls back.
5. **Projection validation moved ahead of the dry-run exit.** Dry-run now parses members, classifies ownership, scans for unregistered projections, and validates all registered projections against the current version, while still mutating nothing.

### Key discovery (verified with real Cargo, offline, in a minimal fixture workspace)

`cargo update -p krometrail --precise X` refreshes the lock entries of the root package AND every version-inheriting workspace member (their effective version changed via `[workspace.package]` inheritance), and leaves an independently versioned member's entry untouched. This is why the old verifier ("every workspace package starts at the product version") passed previous real releases but threw on temporal-vision: its 0.1.1 literal never matched. The hermetic fake-cargo fixtures model exactly this observed projection, and a dedicated real-cargo fixture runs the actual command offline so the model cannot silently drift from Cargo behavior. The fake-cargo verifier also polices fake overreach: if the fake touched independent or unrelated entries, the byte-identity and multiset checks fail the fixture.

### Red-to-green evidence (pre-fix helper vs post-fix helper, same fixtures)

- RED-1 (old helper, mixed-version fixture): `error: Cargo.lock workspace package independent-member did not start at 1.6.2` — the same false-positive class as the reported temporal-vision release failure. GREEN-1 (new helper): prepare succeeds; root + `[workspace.package]` → 1.6.3; lock entries: krometrail and owned-member → 1.6.3, independent-member stays 0.1.1, unrelated-dep stays 5.0.3; independent manifest untouched.
- RED-2 (old helper, projection fixture with `plugin/plugin.json` present): prepare succeeds but `plugin/plugin.json` silently stays at 1.6.2 while registered projections move to 1.6.3 — the omitted Antigravity projection. GREEN-2 (new helper): `plugin/plugin.json` moves to 1.6.3 with everything else; an additionally planted unregistered `plugin/unregistered.json` is refused with `Unregistered shipped version projection(s): plugin/unregistered.json — ...` and zero file mutations.

### Tests added or reworked (all hermetic; no network, tags, pushes, or installation mutation)

- `tests/distribution-static.sh`: inventory-driven fixture generation and assertions (`list_product_projections`, `write_projection_files`, `assert_projections_at`, whole-repo snapshot/diff helpers); reworked plugin-projection fixture (success + full rollback on injected gate failure); new mixed-version workspace fixture covering dry-run non-mutation, successful prepare with exact lock-ownership outcomes, unregistered-projection refusal, drifted-projection refusal, and gate-failure rollback; new real-cargo offline lock-refresh fixture; new static asserts that the helper imports the ownership module and keeps the independent-member checks.
- `tests/plugin-static.sh`: `plugin/plugin.json` (Antigravity manifest) now has the same existence, JSON-validity, and Cargo-identity contracts as the Claude and Codex manifests.
- Real-worktree verification: `bun scripts/bump-version.ts patch --dry-run` reports `Dry run validated 6 registered version projection(s)` and mutates nothing.

### Limitations and notes for parent review

- The scan's shipped-surface boundary is the three inventory-owned directories. A version-bearing file shipped from a new top-level location would need its directory added to `SHIPPED_SURFACE_DIRS`; that decision is deliberately explicit.
- The distribution CI job's Bun dependency already existed; the inventory-driven helpers add `bun` invocations inside the same job only.
- Full Rust quality gate (fmt/check/test/clippy, shared `CARGO_TARGET_DIR`, flock-serialized) was run on this worktree; no product code changed, and results are recorded in the delivery report.

### Parent pre-review correction — 2026-09-05 (db5fff12 → correction commit)

**Exact parent finding.** `hasOwnVersionLiteral()` misclassified legitimate Cargo manifests by equating absence of a double-quoted `version = "…"` regex match with workspace inheritance. Parent reproduced: `Bun.TOML.parse("[package]\nname = 'independent'\nversion = '0.1.1'\n")` yields `package.version = "0.1.1"` while the predicate returned false; separately, an isolated real `cargo metadata --no-deps --offline --format-version 1` accepted a member with no version and reported 0.0.0. So neither a single-quoted literal nor a missing version means inherited/product-owned.

**Correction.** Replaced the regex predicate with `inheritsWorkspaceVersion(manifest)` in `scripts/release-ownership.ts`: it parses the manifest with Bun's TOML parser and returns true only for explicit `[workspace.package]` inheritance — `version.workspace = true` — which dotted keys and inline tables (`version = { workspace = true }`) resolve to identically (`{ workspace = true }`, verified against Bun 1.3.14). Strict `workspace === true`; everything else — double- or single-quoted literals, omitted version (Cargo default 0.0.0), `workspace = false` — classifies independent. The bump helper's call site flipped to the positive form; root package remains the explicit product authority. No parser framework, no dependency, no independent-crate name list, no new regex variants; the old regex predicate is removed entirely. A scratch real-cargo workspace confirmed the classification boundary matches Cargo: dotted and inline-table inheritors both refresh with `cargo update -p krometrail --precise`, while single-quoted (0.1.1) and omitted (0.0.0) members keep their lock entries untouched.

**Red-to-green evidence.** Red (pre-correction helper, extended mixed-version fixture): `error: Cargo.lock product-owned package single-quoted-member did not start at 1.6.2` — the single-quoted independent member was pulled into product-version validation; the ownership truth-table test also refused to pass; the real-cargo fixture would likewise fail because its inline-table inheritor is refreshed by real cargo while the old predicate classified it independent. Green (post-correction): full `tests/distribution-static.sh` passes with the extended fixtures.

**Tests added or extended.** `tests/distribution-static.sh`: mixed hermetic fixture gained `single-quoted-member` (`version = '0.2.2'`) and `omitted-version-member` (no version) with matching hand-written lock entries, asserting both lock entries and manifests stay byte-unchanged and no version is injected into the omitted manifest; a predicate truth-table case list covers dotted inheritance, inline-table inheritance, double-quoted, single-quoted, and omitted versions; real-cargo fixture gained `owned-inline` (`version = { workspace = true }` — real cargo refreshes it to 1.6.3, proving positive recognition matches Cargo), `independent-single` (`version = '0.1.1'`), and `independent-omitted` (defaults 0.0.0), asserting their lock entries and manifests are untouched; new static require pins the helper to `inheritsWorkspaceVersion`. Original ownership/projection/rollback/dry-run/unregistered gates retained unchanged. Docs (`docs/RELEASING.md`, pattern example) and the item's design-decision wording updated from "no literal means inherited" to explicit-inheritance phrasing.

**Limitations.** The predicate trusts Bun's TOML parse of member manifests; a manifest Bun cannot parse fails the member read loudly rather than misclassifying. Strict `workspace === true` means exotic shapes (`workspace = "true"`, nested tables beyond one dotted level) classify independent — the safe direction, since misclassification toward product-owned is the failure mode that corrupts releases. The inheritance question is decided per manifest text; Cargo remains the runtime authority, and the real-cargo fixture is the executable bridge between the two.

### Independent review corrections — 2026-09-05 (06bbf772 → correction commit)

Independent Astra review of the aggregate ba69f404..06bbf772 returned three required P2 corrections; parent accepted all after code/test inspection. Reviewer agent e4084b12-fadc-423 retained for re-review. No product version, tag, push, or publication touched.

**1. Recoupling guard missed the inline TOML shape (required).** The temporal-vision independence guard still regex-checked only dotted `version.workspace = true`, while the classifier accepts `version = { workspace = true }` — so an inline-recoupled temporal-vision would be classified product-owned and moved. Reviewer demonstrated the full drift on an isolated real-Cargo fixture (temporal-vision 1.6.2 → 1.6.3). Correction: the guard now reuses `inheritsWorkspaceVersion(tvCargo)` — one classifier, no second regex — and the diagnostic names `crates/temporal-vision` with the manual-bump instruction. New hermetic fixture: a workspace declaring temporal-vision as a member, recoupled first dotted then inline; each `--dry-run` must fail with the named diagnostic and leave the tree byte-identical. Red-to-green: with the old regex guard restored in a scratch copy, the inline form sailed past the guard and failed later on an unrelated missing-projection error (`Registered product version projection plugin/.claude-plugin/plugin.json is missing`); with the corrected guard it is refused up front with the named diagnostic. docs/RELEASING.md updated to name both TOML shapes.

**2. Mixed-fixture scenario baseline contamination (required).** `snapshot_repo` replaced `.pristine` AFTER planting `plugin/unregistered.json`, so the following restores carried the plant: the registered-drift scenario and the supposed gate-rollback scenario both failed at unregistered detection and never reached their intended failures. Correction: the clean fixture state is snapshotted once into an immutable `.baseline` immediately after build; every scenario restores from it and snapshots its own pre-run state into a separate `.scenario` directory for byte-identity checks; the shared helpers now take the snapshot path explicitly. Each refusal asserts its exact expected error (`Unregistered shipped version projection(s): plugin/unregistered.json …`, `.claude-plugin/marketplace.json must contain exactly one version equal to 1.6.2`), and refusal scenarios assert an empty cargo trace — the fake cargo now appends every invocation to a trace file. The gate-rollback scenario proves the release actually ran: trace contains `update -p krometrail --precise 1.6.3`, `update-manifest-at=version = "1.6.3"` (manifest genuinely rewritten before the lock refresh), and `check --workspace --all-targets --locked`, the bump output contains exactly `Command failed (17): cargo check`, and the final tree is byte-identical to the pre-run snapshot (full reversion). Red-to-green: with the new assertions in place but the contaminated restore order retained, the suite failed at `drifted projection refusal was not the version validation error`; after baseline separation the full suite passes. The simple-workspace rollback fixture additionally asserts the exact gate failure (`Command failed (17): cargo check`) now.

**3. Inventory-versus-reality check was tautological (required).** Fixture creation and assertions both derived from the production registry, so dropping an entry (reviewer dropped Antigravity) left the entire suite green. Correction: a new shipped-surface completeness check byte-copies the real `plugin/`, `.claude-plugin/`, and `.agents/plugins/` trees (copies only; a symlink guard fails the suite if a copy ever contains one, so a test write can never reach the worktree) and checks the registry in both directions with the production scan itself: every registered projection must exist as a real repository file, and the real shipped surface must contain no unregistered version-bearing file. A built-in mutation check re-runs the same scan against a mutated copy of the module with the Antigravity entry removed and requires the scan to report `plugin/plugin.json` — demonstrated live: intact registry scans `[]`, entry-dropped copy scans `["plugin/plugin.json"]`. No second production registry; the production module is never modified.

**Doc correction.** The previously recorded limitation claiming the real-cargo fixture newly introduced a CI Cargo dependency was removed as inaccurate per review.

**Verification.** Full `tests/distribution-static.sh` green (exit 0), including the restructured scenarios; the dry-run check against the real worktree still validates the six registered projections and mutates nothing. Rust gates for this correction (wire-enum scan, fmt, check, clippy, test — shared `CARGO_TARGET_DIR`, flock-serialized, background) were run by the implementation worker; the independent reviewer verified distribution behavior, offline Cargo ownership, all six real projection updates, and dry-run/diff checks separately.

**Limitations.** The completeness check trusts that the three copied directories are the whole shipped surface (the same boundary as `SHIPPED_SURFACE_DIRS`); a projection shipped elsewhere needs its directory registered there, which is the deliberate contract. The mutation check greps the Antigravity path out of a module copy; a registry refactor that renames the path string would need the mutation line updated — the suite fails loudly at the mutation check if the dropped entry is no longer detected, so this cannot silently rot.

### Parent correction — copied Codex catalog layout

Re-review found that multi-source `cp -R` flattened `.agents/plugins` to `repo/plugins`, leaving the real scanner blind to the copied Codex catalog. The parent reproduced the malformed layout and a false-empty scan after removing that catalog's registry entry. The fixture now preserves each surface's full relative path, checks registered-file existence inside the copied tree, and independently removes both Antigravity and Codex catalog entries from module copies to prove detection. The production registry and source files are never mutated by these probes. Parent-run `bash tests/distribution-static.sh` passed in full after correction, including installer/bootstrap/plugin checks and real-Cargo fixtures. Astra accepted the narrow correction, and parent main integration gates passed before closure.

## Parent acceptance and integrated verification

Astra accepted the final catalog-copy correction with no remaining required findings. Parent main-worktree verification passed formatting, wire checks, locked workspace/all-targets check and tests, warning-free default workspace Clippy, the complete distribution/installer/bootstrap/plugin fixture suite, documentation regeneration/build, and diff checks. Receipt: `/tmp/parent-doctor-release-integration.log`, using rustc/cargo 1.96.1 and the serialized shared target. Fixture version changes occurred only in temporary repositories; the actual product remains 1.6.2, and temporal-vision remains independently 0.2.0 from its already accepted provenance change. No tag, publication, release helper transaction against the real repository, or installation mutation was performed.
