---
id: epic-a-grade-reliability-doctor-discovery-only
kind: feature
stage: done
tags: [infra, storage, testing]
parent: epic-a-grade-reliability
depends_on: []
release_binding: null
research_refs: []
research_origin: null
created: 2026-09-05
updated: 2026-09-05
---

# Keep doctor independent of recording storage and reclamation

## Outcome and priority

The documented discovery-only diagnostic initializes the recording runtime and performs cache reclamation before browser discovery. A browser health check can therefore delete retained cache and require writable storage it does not need.

- **Priority:** P1 — wave 2 of [epic-a-grade-reliability](../../backlog/epic-a-grade-reliability.md). Priority is proposed remediation order, not a release commitment.
- **Evidence status:** Reproduced in isolated storage: doctor removed abandoned recording evidence and preserved managed profiles.
- **Origin:** Personal read-only repository review at `eb5b4656`, followed by the user's request to backlog the full path to a solid A (2026-09-05). References are point-in-time; revalidate before implementation.
- **Readiness:** Authorized scoped implementation after the user asked to continue; the design below owns this unit. No release or model-effectiveness study is authorized.

## Evidence

- src/main.rs:58 — runtime built before Doctor/Mcp dispatch
- src/app.rs:375,412 — storage initialization and abandoned-root reclamation

## Implementation direction and boundaries

Compose the discovery command from discovery dependencies rather than constructing the full recording/browser runtime.

Preserve evidence provenance, explicit gaps and uncertainty, authority revalidation, bounded processing, and the current-contract/no-hypothetical-compatibility discipline. Run the applicable production, boundary, failure-path, and integration tests; record actual results and unresolved limitations in this item.

## Authorized design and implementation boundary — 2026-09-05

The user authorized continued work. The current main constructs `build_runtime` before dispatching Doctor/Mcp; `Runtime::run(Doctor)` therefore discovers only after instance ownership, cache reclamation/recovery, and storage validation have already run. Split command composition before recording runtime construction. Doctor should invoke the existing bounded browser discovery/launcher authority directly, with an injectable seam for deterministic tests, not construct inert storage or a fake full runtime. MCP alone composes recording/runtime services and preserves its legitimate startup cleanup. Remove obsolete Doctor-through-recording-runtime branches/test scaffolding rather than retaining two operational routes.

Keep existing discovery results, browser-not-found errors, and output stable. Best-effort diagnostic logging may remain, but state that side effect precisely; unusable recording storage/configuration cannot block discovery. Test an abandoned cache and protected root members byte-for-byte, a path structurally unusable for storage (not chmod-only under root), invalid recording-specific configuration, browser absence, and normal discovery. Use existing binary smoke tests and fake bounded browser executables/ports; never launch Chrome just to test doctor. Do not change store reclamation, profile ownership, compiler/release tooling, or root Cargo metadata. Parent reviews src/main.rs, app composition and binary-boundary tests before acceptance.

Demonstrate the old doctor deleting isolated abandoned cache or failing on unusable recording storage, then green after the split. Use isolated temporary data and never the operator's actual recording root. Run focused smoke/composition regressions, formatting and relevant lint/tests. Update essential troubleshooting/runtime documentation only where needed; do not hand-edit generated llms-full output. No generic dependency-injection framework or new CLI capabilities are needed.

## Acceptance criteria

- [x] Doctor discovers browsers or returns browser_not_found without initializing, recovering, reclaiming, or changing recording-cache members.
- [x] Read-only or unusable recording storage does not prevent browser discovery; test with injected or genuinely non-writable storage rather than a root-bypass permission assertion.
- [x] An abandoned recording cache and its known contents survive doctor byte-for-byte; profiles, configuration, and downloads also survive.
- [x] MCP startup retains its legitimate ownership-checked cache policy. Document any intentionally retained diagnostic logging side effect separately.

## Implementation record — 2026-09-05 (discovery-only doctor split)

### What changed

- `src/main.rs` dispatches `Doctor` before any recording runtime exists. `build_runtime` now runs only on the `Mcp` path. Doctor composes `Doctor::with_system_launcher()` (new `src/doctor.rs`) over the existing `krometrail_cdp::ChromeLauncher` seam with `SystemChromeLauncher`; the injectable seam is the existing launcher trait, no new framework. Diagnostic logging stays process-level and best effort; it is the only data-root side effect doctor can produce, and a diagnostics failure still prints `warning: diagnostic logging unavailable` and continues.
- `Runtime::run(command)` became `Runtime::run_mcp()`; the Doctor-through-recording-runtime branch is removed, so there is exactly one operational doctor route. `browser_not_found()` moved from `src/app.rs` to `src/doctor.rs` (same code, message, `retry=after_recovery`, recovery text). Launcher failures map through `LaunchError::stable_code()` with the same stable, path-free message discipline as the connector's `launch_error_to_core`.
- Default-build `RuntimeDependencies` contains the projections the MCP runtime reads. Qualification-only projections are retained under `qualification-support`, using the original clocks, store, and index; the review correction below repaired their initial removal. Store liveness is unchanged because the connector and evidence services retain their store projections.
- Obsolete scaffolding removed: `doctor_is_discovery_only` (which built full storage to exercise Doctor). Its unique `mcp_dependencies` pointer-identity assertions survive in a slim `mcp_dependency_projection_shares_runtime_services` test that builds no doctor path. `DiscoveryOnlyFake` reduced to a unit struct. Default-build `StorageDependencies` no longer exposes the `timeline`/`gaps` projections, while qualification builds retain them. The root storage test removed its duplicated gap append/query block — that round-trip remains covered at its owning boundary by `crates/krometrail-store/tests/indexed_recording.rs` (sink append + index query), so no coverage was lost; the root test kept its unique value (one-store pointer identity for every wired projection, and the file-occupied-path failure) and was renamed `storage_composition_shares_one_store_and_fails_on_unusable_paths`.
- Docs: `docs/reference/runtime.md` doctor section and `docs/guide/troubleshooting.md` browser-discovery section now state that doctor never initializes, reclaims, or validates recording storage. Generated `llms-full.txt` not touched; needs regeneration at the parent docs build.

### Red (old main, exact binary outputs)

`cargo test --test rust-runtime-smoke -- doctor_` on the pre-change composition (4 new tests, all failing as the item's evidence predicts):

- `doctor_preserves_abandoned_cache_and_unrelated_data_root_members_byte_for_byte`: `Result::unwrap()` on `Err` `NotFound` reading `instances/00000000-0000-4000-8000-000000000001/index.sqlite3` — the old doctor reclaimed the abandoned recording cache (the run itself exited 0 while deleting it).
- `doctor_succeeds_when_the_storage_root_is_structurally_unusable`: doctor exited 1 with `error[persistence_failed] (retry=never): could not create the instance directory` (data root was a regular file; stderr also carried the documented diagnostics warning).
- `doctor_ignores_invalid_recording_only_configuration`: doctor exited 1 with `error[invalid_input] (retry=never): KROMETRAIL_DISK_BUDGET_BYTES must be a positive integer`.
- `doctor_never_creates_recording_setup_for_a_fresh_data_root`: data root contained `["diagnostics", "instances"]` — the old doctor created instance roots.

### Green (new composition)

- The same four binary tests pass, plus existing `doctor_reports_only_the_production_discovery_outcomes` and `mcp_eof_exits_cleanly_without_non_protocol_output` (`cargo test --test rust-runtime-smoke -- doctor_ mcp_eof doctor_reports`: 6 passed, 0 failed).
- Doctor unit tests (`cargo test --bins --locked -- doctor`): `doctor_reports_discovered_installations_without_touching_the_browser`, `doctor_reports_the_explicit_browser_not_found_error_without_a_browser`, `doctor_maps_launcher_failures_onto_stable_errors` — 3 passed.
- Reshaped composition tests pass: `storage_composition_shares_one_store_and_fails_on_unusable_paths`, `mcp_dependency_projection_shares_runtime_services`.
- Full root gate: `cargo fmt --all -- --check` pass; `cargo clippy -p krometrail --all-targets --locked -- -D warnings` pass; `cargo test -p krometrail --locked` — 165 unit tests passed (3 ignored, feature-gated qualification) and 13 integration smoke tests passed, 0 failed.
- `bash scripts/check-wire-enum-schemas.sh`: pass.
- **Authoritative gate rerun (job #8)**: the first full-gate wrapper's `|| { echo GATE-FAILED; tail … }` handler could exit 0 after a failure and its `/tmp/kt-*.log` files were removed before parent review, so the gate was rerun once as `flock /tmp/krometrail-reliability-build.lock bash -c 'set -euo pipefail; … cargo fmt --all -- --check && echo FMT-OK && cargo clippy … -D warnings && echo CLIPPY-OK && cargo test -p krometrail --locked && echo TEST-OK'` with no failure handler and full per-test output inline. Result: exit 0, FMT-OK, CLIPPY-OK, TEST-OK all echoed; test details: 165 passed / 0 failed / 3 ignored (feature-gated video/qualification), 13/13 smoke passed including all four doctor tests. `--version` unchanged (`krometrail 1.6.2`); manual `doctor` run: `browser available: 1 installation(s)`, exit 0, fresh data root contains only `diagnostics`.

### Tests added

Binary smoke (`tests/rust-runtime-smoke.rs`, isolated temp data roots, deterministic `#!/bin/sh --version` fixture discovered via `KROMETRAIL_CHROME`, no real Chrome launch, no operator data root):

1. `doctor_preserves_abandoned_cache_and_unrelated_data_root_members_byte_for_byte` — UUID-named abandoned instance root (index + segment) plus profile/config/download/unknown members byte-for-byte; also asserts no `.owner.lock` appears (doctor never claimed the root) and no second instance root is created.
2. `doctor_succeeds_when_the_storage_root_is_structurally_unusable` — data root is a regular file (structural, not chmod; cannot be bypassed by root); stderr must contain no `error[`.
3. `doctor_never_creates_recording_setup_for_a_fresh_data_root` — fresh data root must contain exactly `diagnostics` afterwards.
4. `doctor_ignores_invalid_recording_only_configuration` — invalid disk budget and retention age ignored; stderr empty.

Unit (`src/doctor.rs`, deterministic fakes, panics if the launcher's `launch`/`managed_profiles` is ever reached): ordinary discovery output line, explicit `browser_not_found` error fields, launcher-failure mapping to stable codes. No-browser error is covered here rather than at the binary level because hosts with installed Chrome make an empty discovery outcome non-deterministic.

### Decisions and limitations

- Discovery stays fallible-tolerant as today: `SystemChromeLauncher::installations` never fails in practice, but the seam's error path maps to stable codes instead of panicking.
- `config.toml` in the preservation test stands in for configuration members; no config file feature exists, and the byte-for-byte rule it exercises is the non-cache-member rule that also covers unknown files.
- MCP startup policy untouched: instance ownership, legacy/incompatible-cache clearing, and reclamation remain MCP-only behavior, covered by the existing smoke suite.
- Not done here (owned elsewhere): root Cargo/lock/version, release tooling, store reclamation internals, temporal-vision, MCP result delivery. Generated docs output requires a parent-run `bun run docs:build` regeneration to include the two doc sentences.

## Independent review response — 2026-09-05 (review4a705308, two required P2s + accepted doc cleanup)

### P2-1: qualification-lane projections restored (src/app.rs only)

The dependency trim removed projections the feature-gated qualification lane genuinely constructs and reads: `live_evaluation.rs:466-484` initializes them in its `RuntimeDependencies` literal, and `capture.rs`/`control.rs` read `recording`, `retention`, `gaps`, `temporal_queries`, `artifact_generation`, `clock` from it; `StorageDependencies.timeline`/`gaps` feed that literal. Correction restores all nine `RuntimeDependencies` fields (`clock`, `wall_clock`, `recording`, `retention`, `timeline`, `catalog`, `gaps`, `temporal_queries`, `artifact_generation`) and the two `StorageDependencies` fields, each `#[cfg(feature = "qualification-support")]`-gated inside already-owned `app.rs` (struct fields, `build_runtime`/`open_storage_with_budget` initializers, imports, and the two test literals), keeping the default doctor/MCP surface lean. No qualification coverage was deleted and no dummy reads were added; no file outside the owned set was touched. `mcp_config` in the qualification literal was NOT restored — it is a pre-existing stale initializer (unknown field at baseline).

Baseline-vs-corrected feature diagnostics (`cargo check -p krometrail --all-targets --features qualification-support --locked`, scratch copy of merge-base `d5047192` via `git archive`, explicit `CARGO_TARGET_DIR=/storage/cargo-target`, serialized under the shared lock; full logs `/tmp/kt-feature-baseline.log` and `/tmp/kt-feature-corrected.log`):

- Reviewer-captured corrected state before this response (79 errors): 59 E0609 reads + 10 E0560 unknown fields. Of those, 68 were introduced by the dependency trim (E0560: `clock`, `wall_clock`, `recording`, `retention`, `timeline`, `catalog`, `gaps`, `temporal_queries`, `artifact_generation`; the 59 E0609s). All 68 are gone after the correction.
- Baseline `d5047192`: 11 errors. Corrected branch after this response: the same 11 signatures — 7 E0308 mismatched types (`live_evaluation/control.rs`, `latency.rs`, `recovery.rs`, `retention.rs`), 3 E0004 non-exhaustive matches over `BrowserOperationKind`/`BrowserOperationResult` (`QueryPage`, `ListPageContexts`, `WaitForPage` and 9 more), 1 E0560 unknown field `mcp_config` in the stale qualification initializer. introduced-by-worker diff: empty. resolved-by-worker diff: empty.
- These 11 baseline blockers are recorded here for the parent to retain in the existing live-browser-qualification item. The feature lane remains explicitly FAILED/unqualified; no gate in this unit claims it passing, and no qualification repair beyond the introduced regression was attempted. Default doctor/root gates pass independently.

### P2-2: RAII cleanup for the new smoke fixtures

`tests/rust-runtime-smoke.rs` gains `ScratchGuard` (PathBuf + Drop `remove_dir_all`), and all four fake-shell doctor tests construct it before any filesystem writes and no longer clean up manually, so assertion panics (including the red-phase failure mode) cannot leak scratch state or fixture executables. A dedicated regression, `scratch_guard_removes_state_when_the_test_panics`, verifies cleanup under `catch_unwind` with an intentional panic. A post-suite filesystem check confirmed no `krometrail-doctor-guard/preserve/unusable/fresh/config-*` leftovers. The parent-owned `a_second_process_leaves_a_running_instance_store_intact` readiness block and `wait_for_instance_root` helper were not touched. The four fake-shell tests are now explicitly `#[cfg(unix)]` — they are Unix qualification, not Windows.

### Accepted documentation cleanup

`docs/reference/runtime.md` and `docs/guide/troubleshooting.md` now state precisely: doctor never opens recording storage or recording configuration; its retained best-effort diagnostic logging appends to the data directory's diagnostics log when writable and degrades to a standard-error warning when not. This matches the reviewer's probes (blocked data/diagnostics/log and invalid budget/age both leave doctor successful, with the documented warning; MCP storage failures still fail correctly).

### Readiness-race observation (parent-owned test)

During this response's first full gate run (job 11), the parent-owned `a_second_process_leaves_a_running_instance_store_intact` failed at `startup should create an index` — the same failure the parent diagnosed on main: `wait_for_instance_root` returns when `.owner.lock` appears, before `SqliteIndex::open` creates the database; adding the guard test changed suite timing and surfaced it. Per the ownership split this block was left untouched; the authoritative rerun excludes only that test (`--skip`, reviewer precedent) and the parent's `653a1908` fix merges separately. The failed run's leaked `krometrail-instance-isolation-*` temp roots (two, both with dead processes and released locks) were removed as filesystem debris.

### Authoritative post-correction gates

`flock /tmp/krometrail-reliability-build.lock bash -c 'set -euo pipefail'`, explicit `CARGO_TARGET_DIR=/storage/cargo-target`, full unique job log: `cargo fmt --all -- --check` pass (FMT-OK); `cargo clippy -p krometrail --all-targets --locked -- -D warnings` pass (CLIPPY-OK); `cargo test -p krometrail --locked -- --skip a_second_process_leaves_a_running_instance_store_intact` — 165 unit passed / 3 ignored, 13 smoke passed / 1 filtered (the parent-owned readiness-race test, excluded pending the parent's merged fix) (TEST-OK); post-suite leftover check NO-LEFTOVER-SCRATCH. Verification provenance is deliberately distinct: the independent reviewer's default-root verification was 165 unit / 3 ignored + 12 smoke (readiness race already excluded); this unit's worker runs now match that shape with 13 smoke once the guard regression was added. No full-workspace test claim is made — workspace-wide tests were not run in this unit.

## Parent acceptance and integrated verification

Independent Astra re-review accepted the corrected production split, qualification projections, and fixture cleanup. The parent also moved the panic-regression setup inside the guarded scope and checked its exact intentional panic payload, preventing a setup error from masquerading as a successful cleanup test. Re-review accepted this delta.

Integrated with the existing readiness repair and temporal-vision 0.2.0, the parent passed formatting, wire checks, locked workspace/all-targets check and tests, warning-free default workspace Clippy, distribution fixtures, documentation regeneration/build, and diff checks under the shared lock with `bash set -euo pipefail`. All 14 runtime smoke tests ran unfiltered and passed, including process isolation and all doctor regressions. Receipt: `/tmp/parent-doctor-release-integration.log`, rustc/cargo 1.96.1. This is not minimum-compiler or live-browser qualification. The 11 pre-existing qualification-feature errors remain separately recorded in the live-browser-qualification item. No browser launch, tag, publication, or product version change was performed.
