# Rule: Kebab-Case Naming

> Files and folders use kebab-case. No layer suffixes (.service, .util, .controller). Descriptive names only.

## Motivation

Consistent naming eliminates guesswork. When every file is kebab-case, you never wonder
whether it's `sessionManager.ts`, `SessionManager.ts`, or `session_manager.ts`. Avoiding
layer suffixes (`.service`, `.util`) keeps file names focused on *what* the code does, not
*what layer* it belongs to — the directory structure already communicates the layer.

## Before / After

### From this codebase: consistent naming

**Good — descriptive kebab-case throughout:**
```
src/core/
  session-manager.ts      (not SessionManager.ts or session_manager.ts)
  dap-client.ts           (not DAPClient.ts or dap.client.ts)
  value-renderer.ts       (not valueRenderer.ts)
  token-budget.ts         (not tokenBudget.ts)

src/adapters/
  helpers.ts              (not adapter-helpers.ts or helpers.util.ts)

src/browser/recorder/
  input-tracker.ts        (not inputTracker.ts)
  event-normalizer.ts     (not event-normalizer.service.ts)
  chrome-launcher.ts      (not ChromeLauncher.ts)
```

### Synthetic example: mixed naming conventions

**Before:**
```
src/
  SessionManager.ts       (PascalCase file)
  api_client.ts           (snake_case file)
  auth.service.ts         (layer suffix)
  stringUtils.ts          (camelCase + layer suffix)
```

**After:**
```
src/
  session-manager.ts
  api-client.ts
  auth.ts                 (directory already says it's a service)
  strings.ts              (or string-helpers.ts if needed)
```

## Exceptions

- **Generated files** may follow their generator's conventions (e.g., `.generated/`).
- **Configuration files** follow ecosystem conventions (`tsconfig.json`, `biome.json`).
- **Acronyms** in file names stay lowercase: `dap-client.ts`, `cdp-adapter.ts` (not
  `DAP-client.ts`).

## Scope

- Applies to: all TypeScript source files and directories under `src/` and `tests/`
- Does NOT apply to: config files, generated files, documentation files
