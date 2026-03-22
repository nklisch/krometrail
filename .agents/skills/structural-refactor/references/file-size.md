# Rule: File Size

> Files should stay under ~500 lines; evaluate for splitting above that.

## Motivation

Large files are harder to navigate, review, and reason about. When a file exceeds 500 lines,
it usually contains multiple concepts that could be separate modules. The 500-line threshold
is a soft signal to consider whether the file is doing too many things, not a hard limit.

## Before / After

### From this codebase: session-manager.ts

**Before:** (`src/core/session-manager.ts` — was 1,365 lines, split in March 2026)
```
src/core/
  session-manager.ts   (1,365 lines — session lifecycle, action dispatch,
                        viewport rendering, watch expressions, output capture,
                        compression, session logging)
```

**After:** (implemented)
```
src/core/
  session-manager.ts        (lifecycle, coordination, viewport rendering)
  breakpoint-manager.ts     (breakpoint operations)
  execution-controller.ts   (continue, step, runTo)
  state-inspector.ts        (evaluate, variables, stack trace, source)
  session-output.ts         (output capture, session log, watch expressions)
```

### Synthetic example: monolithic tool handler

**Before:**
```
src/tools/
  api-tools.ts    (800 lines — 12 tool handlers + schemas + helpers)
```

**After:**
```
src/tools/
  api-tools.ts    (tool registration + shared helpers)
  api-query.ts    (query-related tool handlers)
  api-mutate.ts   (mutation-related tool handlers)
```

## Exceptions

- **Registration files** (`mcp/tools/index.ts`, `cli/commands/index.ts`) — sequential tool/command
  definitions are naturally long. Splitting by individual tool adds file sprawl without benefit.
- **Injection scripts** (`browser/recorder/framework/*-injection.ts`) — template literals generating
  browser-side JavaScript. Splitting fragments the injection logic.
- **Protocol definitions** (`daemon/protocol.ts`) — type collections that belong together as a
  single reference document.
- **Shell completion generators** (`cli/commands/completions.ts`) — string template builders
  that are procedural by nature.

## Scope

- Applies to: all TypeScript source files under `src/`
- Does NOT apply to: test files, generated files, fixture files
