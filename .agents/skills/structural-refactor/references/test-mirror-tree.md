# Rule: Test Mirror Tree

> Tests in tests/{unit,integration,e2e}/ mirroring src/ structure. Never co-locate tests next to source files.

## Motivation

A separate test tree keeps the src/ directory focused on production code. The mirror structure
makes test files easy to find — `src/core/viewport.ts` maps to
`tests/unit/core/viewport.test.ts`. This also enables different test configurations per tier
(unit tests mock freely, integration tests use real debuggers, e2e tests are end-to-end).

## Before / After

### From this codebase: current test layout

**Good — clean mirror structure:**
```
tests/
├── unit/                 (mirrors src/, mocks OK)
│   ├── adapters/
│   ├── browser/
│   ├── cli/
│   ├── core/
│   ├── daemon/
│   ├── frameworks/
│   └── mcp/
├── integration/          (mirrors src/, real debuggers, no mocks)
│   ├── adapters/
│   └── browser/
├── e2e/                  (mirrors src/, full MCP/CLI-to-viewport)
│   ├── adapters/
│   └── browser/
├── fixtures/             (real programs used as debug targets)
└── agent-harness/        (scenario-based evaluation suite)
```

### Synthetic example: co-located tests (anti-pattern)

**Before:**
```
src/core/
  viewport.ts
  viewport.test.ts       (test next to source — clutters src/)
  viewport.spec.ts       (another test convention mixed in)
```

**After:**
```
src/core/
  viewport.ts
tests/unit/core/
  viewport.test.ts       (test in mirror tree)
```

## Exceptions

- **Fixture files** live in `tests/fixtures/`, not mirrored from src/.
- **Test utilities** (factories, helpers) live in `tests/` at the appropriate level,
  not mirrored from a src/ location.
- **Agent harness scenarios** live in `tests/agent-harness/scenarios/` — these are
  evaluation programs, not unit tests.

## Scope

- Applies to: all test files (*.test.ts, *.spec.ts)
- Does NOT apply to: fixtures, test utilities, scenario files
