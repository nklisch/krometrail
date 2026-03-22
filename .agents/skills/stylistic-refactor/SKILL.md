---
name: stylistic-refactor
description: >
  Project stylistic refactoring rules for TypeScript/Bun. Proactively scans for refactoring
  opportunities and produces a prioritized plan. Defines the team's preferred coding style.
user-invocable: true
allowed-tools: Read, Glob, Grep, Bash, Agent, Write
---

# Stylistic Refactor

Scan the codebase for opportunities to apply these stylistic preferences.
Each style has a reference file with rationale, examples, and exceptions.

## Styles

| Style | Rule (one line) | Reference |
|-------|-----------------|-----------|
| Early Returns | Prefer early returns and guard clauses over nested conditionals | [details](references/early-returns.md) |
| Classes vs Functions | Classes for stateful coordinators; plain functions for pure logic | [details](references/classes-vs-functions.md) |
| Arrow vs Declaration | Arrow functions for callbacks/inline; function declarations for named exports | [details](references/arrow-vs-declaration.md) |
| Explicit Return Types | Exported functions must have explicit return type annotations | [details](references/explicit-return-types.md) |
| Descriptive Naming | Boolean prefixes (is/has/should/can); descriptive names over abbreviations | [details](references/descriptive-naming.md) |
| Function Size | Functions should stay under ~50 lines; extract when logic is a named concept | [details](references/function-size.md) |
| Composition over Inheritance | Flat interface implementations; no deep class hierarchies; share via helpers | [details](references/composition-over-inheritance.md) |

## Output

Write the refactoring plan to `docs/stylistic-refactor-plan.md`.

The document should be a **prioritized refactoring plan** with these sections:

### High Value
Implement-ready refactors with current/target code, file paths, and acceptance criteria.

### Worth Considering
Valid refactors with moderate impact. Brief entries with file paths and rationale.

### Not Worth It
Code that technically violates a style but should NOT be refactored, with justification.
