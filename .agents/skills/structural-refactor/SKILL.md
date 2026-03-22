---
name: structural-refactor
description: >
  Project structural organization rules for TypeScript/Bun. Proactively scans for organizational
  issues and produces a prioritized plan. Defines the team's preferred file, folder, and module
  structure.
user-invocable: true
allowed-tools: Read, Glob, Grep, Bash, Agent, Write
---

# Structural Refactor

Scan the codebase for organizational issues based on these structural rules.
Each rule has a reference file with rationale, examples, and exceptions.

## Rules

| Rule | Summary (one line) | Reference |
|------|-------------------|-----------|
| File Size | Files should stay under ~500 lines; evaluate for splitting above that | [details](references/file-size.md) |
| Curated Barrels | Index files selectively re-export public APIs; no splat exports | [details](references/curated-barrels.md) |
| Import Direction | Strict layer hierarchy: cli/mcp → daemon → core → domains | [details](references/import-direction.md) |
| Centralized Types | Each domain has at most one types.ts; Zod schemas live alongside types | [details](references/centralized-types.md) |
| Test Mirror Tree | Tests in tests/{unit,integration,e2e}/ mirroring src/ structure | [details](references/test-mirror-tree.md) |
| Docs Hierarchy | Foundation docs authoritative; designs/ historical; legacy/ deprecated | [details](references/docs-hierarchy.md) |
| Kebab-Case Naming | Files and folders use kebab-case; no layer suffixes (.service, .util) | [details](references/kebab-case-naming.md) |

## Output

Write the refactoring plan to `docs/structural-refactor-plan.md`.

The document should be a **prioritized refactoring plan** with these sections:

### High Value
Implement-ready structural changes with current/target layout, file paths, and acceptance criteria.

### Worth Considering
Valid reorganizations with moderate impact. Brief entries with file paths and rationale.

### Not Worth It
Code that technically violates a rule but should NOT be reorganized, with justification.
