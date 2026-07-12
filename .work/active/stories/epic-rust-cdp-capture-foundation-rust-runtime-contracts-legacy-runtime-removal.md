---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal
kind: story
stage: implementing
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

- [ ] The remote tag gate and pre-teardown Rust gate are recorded and pass.
- [ ] No TypeScript product runtime, DAP adapter/command, old daemon, framework recorder, or product test remains buildable.
- [ ] Every retained browser fixture has a documented current foundation use.
- [ ] Rust fmt/check/test/clippy stay green after deletion.
- [ ] Repository search finds no executable path to the old runtime.
