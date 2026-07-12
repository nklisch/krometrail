---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal
kind: story
stage: review
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Remove the verified legacy product runtime

## Scope

After the Rust pre-teardown gate is green, reverify remote tag `v0.2.20` at `3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558`, then remove the TypeScript/DAP product runtime and every test, benchmark, fixture, harness, and config whose only contract is that runtime.

Do not delete all test assets wholesale. Preserve only browser target applications that can exercise current browser-control or temporal-evaluation intent, documenting a current use for each retained fixture. The parent feature's Unit 5 defines the path classification and cutover safety rules.

## Required gate

```text
remote refs/tags/v0.2.20 == 3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558
AND Rust fmt/check/test/clippy == green
```

## Implementation requirements

- Record the exact remote verification command/output immediately before deletion.
- Remove `src/**/*.ts`, old product suites, DAP fixtures/helpers/harness, agent debugger harness, DAP benchmarks, and unused TypeScript runtime config.
- Classify tracked paths before deletion; do not use an indiscriminate `rm -rf tests benchmarks` operation.
- Retained browser fixtures are target applications/dev assets, not a second product runtime.
- Do not port compatibility types or stale contracts into Rust.

## Acceptance criteria

- [x] The remote tag gate and pre-teardown Rust gate are recorded and pass.
- [x] No TypeScript product runtime, DAP adapter/command, old daemon, framework recorder, or product test remains buildable.
- [x] Every retained browser fixture has a documented current foundation use.
- [x] Rust fmt/check/test/clippy stay green after deletion.
- [x] Repository search finds no executable path to the old runtime.

## Implementation notes

- Dependency verification: `.work/bin/work-view --stage done --paths` listed `epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root.md` at terminal done before teardown.
- Exact destructive gate command/output immediately before deletion:
  ```text
  $ git ls-remote --tags origin refs/tags/v0.2.20
  3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558	refs/tags/v0.2.20
  $ cargo fmt --all -- --check
  PASS cargo fmt --all -- --check
  $ cargo check --workspace --all-targets
      Finished `dev` profile
  $ cargo test --workspace --all-targets
  29 passed (26 core unit tests + 3 Rust runtime smoke tests)
  $ cargo clippy --workspace --all-targets --all-features -- -D warnings
      Finished `dev` profile
  PRE-TEARDOWN GATE: PASS
  ```
- Path classification was performed with `git ls-files` before deletion: 106 TypeScript product files under `src/`; 420 old unit/integration/e2e, debugger-harness, harness, and helper files; 30 non-browser DAP fixtures; 3 DAP benchmark files; and 5 obsolete TypeScript/test configs. The 66 tracked browser target files were retained, alongside the classification manifest. No recursive `rm -rf` was used.
- Removed categories: all `src/**/*.ts`; old product tests and helpers; `tests/agent-harness/`; `tests/harness/`; non-browser `tests/fixtures/{bun,cpp,csharp,go,kotlin,launch-json,node,python,ruby,swift}/`; `benchmarks/`; `tsconfig.json`; `vitest.config.ts`; `biome.json`; `Dockerfile.test`; `tap.json`; and the old runtime-only `scripts/generate-docs.ts` and `scripts/setup-test-deps.sh`.
- Retained fixture classification: `tests/fixtures/browser/README.md` documents current browser-control/temporal-evaluation use for `react-bugs`, `react-counter`, `react-spa`, `simple-page`, `test-app`, `vue3-counter`, `vue3-pinia`, `vue-bugs`, and `vue-spa`. They remain target applications only, not framework-observation runtime contracts.
- Root `package.json` is now private docs/fixture tooling only: obsolete product entry fields, Bun compile/MCP/test/lint commands, and DAP dependencies were removed; VitePress docs commands remain. No Rust compatibility types or legacy contracts were copied.
- Post-teardown verification: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets` (29 passed), and `cargo clippy --workspace --all-targets --all-features -- -D warnings` all pass. `git ls-files 'src/**/*.ts'` and all classified legacy roots return no paths; stale executable scan over `package.json`, `scripts/`, `src/`, `crates/`, and `tests/` returns no old runtime entrypoint/import; `git diff --check` passes.
- Files changed: deleted the classified legacy runtime/test/config paths above; updated `package.json`; added `tests/fixtures/browser/README.md`; updated this story.
- Tests added: none; retained Rust tests remain authoritative.
- Discrepancies from design: none. The root package cleanup was included because its prior `bin`/`main`/test commands were executable paths to the removed runtime, while docs-only VitePress tooling remains for the later documentation cutover.
- Adjacent issues parked: none.
