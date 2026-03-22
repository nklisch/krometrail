# Rule: Centralized Types

> Each domain has at most one types.ts file. Zod schemas live alongside their type definitions.

## Motivation

Centralizing types per domain makes them discoverable — developers know exactly where to look
for a type definition. Co-locating Zod schemas with types ensures the schema and type stay in
sync (Zod infers the type from the schema). Scattering type-only files across a domain makes
it hard to find definitions and leads to duplicate or conflicting types.

## Before / After

### From this codebase: current type locations

**Good — each domain has one types file:**
```
src/core/types.ts              (237 lines — ViewportConfig, Breakpoint, CompressionTier)
src/core/enums.ts              (151 lines — StepDirection, VariableScope, EventType)
src/browser/types.ts           (66 lines — RecordedEvent, Marker, BrowserSessionInfo)
src/browser/executor/types.ts  (224 lines — StepRequest, StepResult, CaptureConfig)
src/adapters/base.ts           (63 lines — DebugAdapter interface, LaunchConfig)
```

**Schema co-location** (in `src/core/types.ts`):
```typescript
export const ViewportConfigSchema = z.object({
  sourceContextLines: z.number().default(15),
  stackDepth: z.number().default(5),
  // ...
});
export type ViewportConfig = z.infer<typeof ViewportConfigSchema>;
```

### Synthetic example: scattered types

**Before:**
```
src/auth/
  login-types.ts       (LoginRequest, LoginResponse)
  session-types.ts     (SessionData, SessionToken)
  middleware-types.ts   (AuthMiddlewareConfig)
```

**After:**
```
src/auth/
  types.ts             (all auth types + schemas in one file)
```

## Exceptions

- **Interface contracts** (like `src/adapters/base.ts`) may live in a `base.ts` file rather
  than `types.ts` when they define the primary abstraction of the module.
- **Protocol definitions** (`src/daemon/protocol.ts`) may contain types alongside RPC
  method definitions — this is acceptable as a single reference document.
- **If a types.ts exceeds ~400 lines**, consider splitting by sub-domain (e.g.,
  `browser/types.ts` and `browser/executor/types.ts` is fine).

## Scope

- Applies to: all domains under `src/`
- Does NOT apply to: test type helpers, generated types, third-party type augmentations
