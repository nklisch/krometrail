# Rule: Import Direction

> Strict layer hierarchy: cli/mcp → daemon → core → domains. Domains never import from entry points or orchestration layers.

## Motivation

A clear import direction prevents circular dependencies and makes the codebase easier to
reason about. When every file imports "downward" in the hierarchy, you can understand any
module by reading it and its dependencies — never worrying about hidden upward coupling.
This also enables safe refactoring: changes to cli/ never break adapters/.

## Before / After

### From this codebase: the actual hierarchy

```
cli/  ──────────────────┐
mcp/  ──────────────────┤ entry points (import everything below)
                        ↓
daemon/ ────────────────┤ orchestration (import core + domains)
                        ↓
core/   ────────────────┤ shared logic (import nothing above)
                        ↓
adapters/ ──────┐
browser/  ──────┤       domains (import core/ only, never each other
frameworks/ ────┘        unless explicitly justified)
```

**Good:** `src/adapters/python.ts` imports from `../core/errors.js` (downward to core)
**Good:** `src/mcp/tools/index.ts` imports from `../../core/types.js` (entry to core)
**Bad:** If `src/adapters/python.ts` imported from `../cli/format.js` (domain to entry point)

### Synthetic example: upward import violation

**Before:**
```typescript
// src/adapters/node.ts
import { formatOutput } from "../cli/format.js";  // BAD: domain → entry point
```

**After:**
```typescript
// Move shared logic to core/
// src/core/format-helpers.ts
export function formatOutput(...) { ... }

// src/adapters/node.ts
import { formatOutput } from "../core/format-helpers.js";  // GOOD: domain → core
```

## Exceptions

- **core/ importing from adapters/base.ts** is acceptable — the adapter interface definition
  is a contract, not an implementation detail.
- **Cross-domain imports within browser/** (e.g., investigation/ → storage/) are acceptable —
  browser/ is a single domain with internal sub-modules.
- **Test files** may import from any layer for testing purposes.

## Scope

- Applies to: all TypeScript source files under `src/`
- Does NOT apply to: test files, build scripts, config files
