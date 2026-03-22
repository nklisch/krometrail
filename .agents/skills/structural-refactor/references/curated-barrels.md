# Rule: Curated Barrels

> Index files selectively re-export public APIs; no splat exports. Only create barrels where there's a clear module boundary.

## Motivation

Barrel files define module boundaries. `export *` re-exports leak internal implementation
details, increase bundle size, and make it impossible to tell what's public from the import
site. Curated barrels explicitly declare the public contract of a module, making it easier to
refactor internals without breaking consumers.

## Before / After

### From this codebase: library public API

**Good barrel** (`src/index.ts`):
```typescript
// Curated: only type exports + one render function
export type { DebugAdapter, DAPConnection, LaunchConfig } from "./adapters/base.js";
export type { ViewportConfig } from "./core/types.js";
export { renderViewport } from "./core/viewport.js";
```

**Good barrel** (`src/frameworks/index.ts`):
```typescript
// Curated: registry functions + detection interface
export { registerAllDetectors, detectFramework, listDetectors } from "./registry.js";
export type { FrameworkOverrides } from "./types.js";
```

### Synthetic example: splat export anti-pattern

**Before:**
```typescript
// src/utils/index.ts — leaks everything
export * from "./strings.js";
export * from "./dates.js";
export * from "./validation.js";
export * from "./internal-helpers.js";  // internal detail leaked
```

**After:**
```typescript
// src/utils/index.ts — curated public API
export { formatDate, parseDate } from "./dates.js";
export { slugify, truncate } from "./strings.js";
export { validateEmail, validateUrl } from "./validation.js";
// internal-helpers.js intentionally NOT exported
```

## Exceptions

- **Directories without a module boundary** don't need a barrel file. For example,
  `src/adapters/` has no index.ts — callers import from `registry.ts` directly, which is
  the natural entry point. Not every folder needs a barrel.
- **Entry point files** (`src/cli/index.ts`, `src/mcp/index.ts`) that bootstrap the application
  are not barrels — they're executables.

## Scope

- Applies to: all `index.ts` files under `src/`
- Does NOT apply to: entry points (cli, mcp), test files, generated files
