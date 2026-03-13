# Design: Type Boundary Consolidation

## Overview

Eliminate duplicated string literal enums and Zod schemas across the codebase by establishing single-source-of-truth const tuples in a shared enums module. All downstream consumers (daemon protocol, MCP tools, CLI, browser subsystem) derive their types and schemas from these central definitions.

This also fixes two active bugs where the daemon protocol schemas are missing values that the MCP tools accept (`"framework"` in overview include, `"framework_state"` in diff include).

---

## Problem Inventory

| Enum-like Value | Duplication Sites | Risk |
|---|---|---|
| Framework names | `browser/types.ts:22`, `daemon/protocol.ts:247`, `mcp/tools/browser.ts:84`, `mcp/tools/browser.ts:276`, `browser/recorder/framework/index.ts:28` | 5 sites |
| Step direction | `session-manager.ts:637`, `daemon/protocol.ts:141`, `mcp/tools/index.ts:255`, `cli/commands/index.ts:304-305` | 4 sites |
| Variable scope | `daemon/protocol.ts:198`, `mcp/tools/index.ts:397` | 2 sites |
| Output stream | `daemon/protocol.ts:240`, `mcp/tools/index.ts:473` | 2 sites |
| Session log format | `daemon/protocol.ts:234`, `mcp/tools/index.ts:462` | 2 sites |
| Severity | `browser/types.ts:52,71`, `browser/recorder/auto-detect.ts:11,164,166`, `browser/recorder/rolling-buffer.ts:28` | 6 sites |
| EventType | `browser/types.ts:1-18` (TS union), `mcp/tools/browser.ts:245-258` (Zod enum) | 2 sites (TS vs Zod) |
| ActionObservation kind | `core/types.ts:181` (TS union), `core/session-logger.ts` (hardcoded strings) | 2 sites |
| Overview include | `daemon/protocol.ts:283` (missing `"framework"`), `mcp/tools/browser.ts:207`, `query-engine.ts:417` | **BUG** — daemon rejects `"framework"` |
| Diff include | `daemon/protocol.ts:316` (missing `"framework_state"`), `mcp/tools/browser.ts:347` | **BUG** — daemon rejects `"framework_state"` |
| Inspect include | `daemon/protocol.ts:306`, `mcp/tools/browser.ts:318`, `query-engine.ts:446` | 3 sites |
| Replay format | `daemon/protocol.ts:325`, `mcp/tools/browser.ts:376`, `replay-context.ts:4` | 3 sites |
| Test framework | `daemon/protocol.ts:326`, `mcp/tools/browser.ts:378`, `replay-context.ts:5` | 3 sites |
| Export format | `daemon/protocol.ts:332` | 1 site (fine) |
| Watch action | `mcp/tools/index.ts:438` | 1 site (fine) |
| Launch request type | `core/launch-json.ts:44` | 1 site (fine) |
| ViewportConfig fields | `core/types.ts:5-12` (camelCase+defaults), `daemon/protocol.ts:118-127` (camelCase, no defaults, inline), `mcp/tools/index.ts:11-21` (snake_case) | 3 definitions |
| SessionStatus vs SessionState | `core/types.ts:69` vs `session-manager.ts:99` — `SessionState` adds `"launching"` | semantic overlap |

---

## Implementation Units

### Unit 1: Create `src/core/enums.ts` — Central Enum Registry

**File**: `src/core/enums.ts`

```typescript
import { z } from "zod";

// --- Debug Enums ---

export const STEP_DIRECTIONS = ["over", "into", "out"] as const;
export const StepDirectionSchema = z.enum(STEP_DIRECTIONS);
export type StepDirection = z.infer<typeof StepDirectionSchema>;

export const VARIABLE_SCOPES = ["local", "global", "closure", "all"] as const;
export const VariableScopeSchema = z.enum(VARIABLE_SCOPES);
export type VariableScope = z.infer<typeof VariableScopeSchema>;

export const OUTPUT_STREAMS = ["stdout", "stderr", "both"] as const;
export const OutputStreamSchema = z.enum(OUTPUT_STREAMS);
export type OutputStream = z.infer<typeof OutputStreamSchema>;

export const SESSION_LOG_FORMATS = ["summary", "detailed"] as const;
export const SessionLogFormatSchema = z.enum(SESSION_LOG_FORMATS);
export type SessionLogFormat = z.infer<typeof SessionLogFormatSchema>;

export const SESSION_STATUSES = ["running", "stopped", "terminated", "error"] as const;
export const SessionStatusSchema = z.enum(SESSION_STATUSES);
export type SessionStatus = z.infer<typeof SessionStatusSchema>;

export const SESSION_STATES = ["launching", ...SESSION_STATUSES] as const;
export const SessionStateSchema = z.enum(SESSION_STATES);
export type SessionState = z.infer<typeof SessionStateSchema>;

export const STOP_REASONS = ["breakpoint", "step", "exception", "pause", "entry"] as const;
export const StopReasonSchema = z.enum(STOP_REASONS);
export type StopReason = z.infer<typeof StopReasonSchema>;

// --- Browser Enums ---

export const FRAMEWORKS = ["react", "vue", "solid", "svelte"] as const;
export const FrameworkSchema = z.enum(FRAMEWORKS);
export type Framework = z.infer<typeof FrameworkSchema>;

export const SEVERITIES = ["low", "medium", "high"] as const;
export const SeveritySchema = z.enum(SEVERITIES);
export type Severity = z.infer<typeof SeveritySchema>;

export const EVENT_TYPES = [
	"navigation",
	"network_request",
	"network_response",
	"console",
	"page_error",
	"user_input",
	"dom_mutation",
	"form_state",
	"screenshot",
	"performance",
	"websocket",
	"storage_change",
	"marker",
	"framework_detect",
	"framework_state",
	"framework_error",
] as const;
export const EventTypeSchema = z.enum(EVENT_TYPES);
export type EventType = z.infer<typeof EventTypeSchema>;

/** Subset of EventType used in search filters (excludes dom_mutation, form_state, storage_change). */
export const SEARCHABLE_EVENT_TYPES = [
	"navigation",
	"network_request",
	"network_response",
	"console",
	"page_error",
	"user_input",
	"websocket",
	"performance",
	"marker",
	"framework_detect",
	"framework_state",
	"framework_error",
] as const;
export const SearchableEventTypeSchema = z.enum(SEARCHABLE_EVENT_TYPES);

export const FRAMEWORK_CHANGE_TYPES = ["mount", "update", "unmount", "store_mutation"] as const;
export const FrameworkChangeTypeSchema = z.enum(FRAMEWORK_CHANGE_TYPES);
export type FrameworkChangeType = z.infer<typeof FrameworkChangeTypeSchema>;

export const ACTION_OBSERVATION_KINDS = [
	"unexpected_value",
	"variable_changed",
	"new_frame",
	"exception",
	"bp_hit",
	"terminated",
] as const;
export const ActionObservationKindSchema = z.enum(ACTION_OBSERVATION_KINDS);
export type ActionObservationKind = z.infer<typeof ActionObservationKindSchema>;

// --- Browser Investigation Enums ---

export const OVERVIEW_INCLUDES = ["timeline", "markers", "errors", "network_summary", "framework"] as const;
export const OverviewIncludeSchema = z.enum(OVERVIEW_INCLUDES);
export type OverviewInclude = z.infer<typeof OverviewIncludeSchema>;

export const INSPECT_INCLUDES = ["surrounding_events", "network_body", "screenshot", "form_state", "console_context"] as const;
export const InspectIncludeSchema = z.enum(INSPECT_INCLUDES);
export type InspectInclude = z.infer<typeof InspectIncludeSchema>;

export const DIFF_INCLUDES = ["form_state", "storage", "url", "console_new", "network_new", "framework_state"] as const;
export const DiffIncludeSchema = z.enum(DIFF_INCLUDES);
export type DiffInclude = z.infer<typeof DiffIncludeSchema>;

export const REPLAY_FORMATS = ["summary", "reproduction_steps", "test_scaffold"] as const;
export const ReplayFormatSchema = z.enum(REPLAY_FORMATS);
export type ReplayFormat = z.infer<typeof ReplayFormatSchema>;

export const TEST_FRAMEWORKS = ["playwright", "cypress"] as const;
export const TestFrameworkSchema = z.enum(TEST_FRAMEWORKS);
export type TestFramework = z.infer<typeof TestFrameworkSchema>;

export const EXPORT_FORMATS = ["har"] as const;
export const ExportFormatSchema = z.enum(EXPORT_FORMATS);
export type ExportFormat = z.infer<typeof ExportFormatSchema>;

// --- ViewportConfig shared field shape ---
// Used by daemon protocol (camelCase) and core types.
// MCP tools derive a snake_case version from these keys.

export const VIEWPORT_CONFIG_FIELDS = [
	"sourceContextLines",
	"stackDepth",
	"localsMaxDepth",
	"localsMaxItems",
	"stringTruncateLength",
	"collectionPreviewItems",
] as const;

/** Zod schema for optional camelCase viewport config (used by daemon protocol). */
export const ViewportConfigPartialSchema = z.object({
	sourceContextLines: z.number().optional(),
	stackDepth: z.number().optional(),
	localsMaxDepth: z.number().optional(),
	localsMaxItems: z.number().optional(),
	stringTruncateLength: z.number().optional(),
	collectionPreviewItems: z.number().optional(),
});
```

**Implementation Notes**:
- Every `as const` tuple is the single source of truth
- Zod schemas derive from the tuple via `z.enum(TUPLE)`
- TypeScript types derive from Zod via `z.infer`
- Adding a new framework/event type/etc. requires changing only this file
- `SESSION_STATES` extends `SESSION_STATUSES` by prepending `"launching"` — this makes explicit that `SessionState` is a superset of `SessionStatus`

**Acceptance Criteria**:
- [ ] File exports all listed const tuples, Zod schemas, and inferred types
- [ ] `SessionState` is a superset of `SessionStatus` (`SESSION_STATES` includes all of `SESSION_STATUSES` plus `"launching"`)
- [ ] No Zod import errors — uses `zod` v4 `z.enum()` with const tuple
- [ ] All types are re-exported from `src/index.ts` as needed

---

### Unit 2: Update `src/core/types.ts` — Import from enums

**File**: `src/core/types.ts`

```typescript
// Replace inline definitions with imports from enums.ts
import {
	type ActionObservationKind,
	type SessionStatus,
	type StopReason,
} from "./enums.js";

// Re-export for backward compatibility
export type { SessionStatus, StopReason } from "./enums.js";
```

**Changes**:
1. Remove `SessionStatus` type alias (line 69) — re-export from `enums.js`
2. Remove `StopReason` type alias (line 71) — re-export from `enums.js`
3. Change `ActionObservation.kind` from inline union to `ActionObservationKind` type
4. Keep `ViewportConfigSchema` with `.default()` values — this is the "full config with defaults" schema; `ViewportConfigPartialSchema` in enums is the "optional override" version used by daemon/MCP

**Acceptance Criteria**:
- [ ] `SessionStatus` and `StopReason` are no longer defined inline — they come from `enums.ts`
- [ ] `ActionObservation.kind` uses `ActionObservationKind` type
- [ ] Existing imports of `SessionStatus`, `StopReason` from `core/types.ts` continue to work (re-export)
- [ ] `ViewportConfigSchema` retains `.default()` values

---

### Unit 3: Update `src/core/session-manager.ts` — Import enums

**File**: `src/core/session-manager.ts`

**Changes**:
1. Replace `SessionState` type alias (line 99) with import from `enums.ts`
2. Replace inline `"over" | "into" | "out"` in `step()` signature (line 637) with `StepDirection` type
3. Import `StepDirection`, `SessionState` from `../core/enums.js`

```typescript
// Before
export type SessionState = "launching" | "running" | "stopped" | "terminated" | "error";
// ...
async step(sessionId: string, direction: "over" | "into" | "out", count = 1, threadId?: number): Promise<string> {

// After
import { type SessionState, type StepDirection } from "./enums.js";
export type { SessionState }; // re-export for consumers
// ...
async step(sessionId: string, direction: StepDirection, count = 1, threadId?: number): Promise<string> {
```

**Acceptance Criteria**:
- [ ] `SessionState` is imported from enums, not defined inline
- [ ] `step()` uses `StepDirection` type
- [ ] Existing consumers of `SessionState` from session-manager continue to work

---

### Unit 4: Update `src/daemon/protocol.ts` — Import enums, fix bugs

**File**: `src/daemon/protocol.ts`

**Changes**:
1. Import all enum schemas from `enums.ts`
2. Replace inline `z.enum(["over", "into", "out"])` in `StepParamsSchema` with `StepDirectionSchema`
3. Replace inline `z.enum(["local", "global", "closure", "all"])` in `VariablesParamsSchema` with `VariableScopeSchema`
4. Replace inline `z.enum(["summary", "detailed"])` in `SessionLogParamsSchema` with `SessionLogFormatSchema`
5. Replace inline `z.enum(["stdout", "stderr", "both"])` in `OutputParamsSchema` with `OutputStreamSchema`
6. Replace inline `z.enum(["react", "vue", "solid", "svelte"])` in `FrameworkStateConfigSchema` with `FrameworkSchema`
7. **BUG FIX**: Replace inline `z.enum(["timeline", "markers", "errors", "network_summary"])` in `BrowserOverviewParamsSchema` with `OverviewIncludeSchema` — adds missing `"framework"`
8. **BUG FIX**: Replace inline `z.enum(["form_state", "storage", "url", "console_new", "network_new"])` in `BrowserDiffParamsSchema` with `DiffIncludeSchema` — adds missing `"framework_state"`
9. Replace inline `z.enum(["surrounding_events", ...])` in `BrowserInspectParamsSchema` with `InspectIncludeSchema`
10. Replace inline `z.enum(["summary", "reproduction_steps", "test_scaffold"])` in `BrowserReplayContextParamsSchema` with `ReplayFormatSchema`
11. Replace inline `z.enum(["playwright", "cypress"])` in `BrowserReplayContextParamsSchema` with `TestFrameworkSchema`
12. Replace inline `z.enum(["har"])` in `BrowserExportParamsSchema` with `ExportFormatSchema`
13. Replace inline viewportConfig z.object in `LaunchParamsSchema` and `AttachParamsSchema` with `ViewportConfigPartialSchema`

```typescript
// Before (protocol.ts:139-144)
export const StepParamsSchema = z.object({
	sessionId: z.string(),
	direction: z.enum(["over", "into", "out"]),
	count: z.number().optional(),
	threadId: z.number().optional(),
});

// After
import { StepDirectionSchema, VariableScopeSchema, /* ... */ } from "../core/enums.js";

export const StepParamsSchema = z.object({
	sessionId: z.string(),
	direction: StepDirectionSchema,
	count: z.number().optional(),
	threadId: z.number().optional(),
});
```

```typescript
// Before (protocol.ts:281-287) — MISSING "framework"
export const BrowserOverviewParamsSchema = z.object({
	sessionId: z.string(),
	include: z.array(z.enum(["timeline", "markers", "errors", "network_summary"])).optional(),
	// ...
});

// After — fixed via OverviewIncludeSchema which includes "framework"
export const BrowserOverviewParamsSchema = z.object({
	sessionId: z.string(),
	include: z.array(OverviewIncludeSchema).optional(),
	// ...
});
```

```typescript
// Before (protocol.ts:312-318) — MISSING "framework_state"
export const BrowserDiffParamsSchema = z.object({
	sessionId: z.string(),
	before: z.string(),
	after: z.string(),
	include: z.array(z.enum(["form_state", "storage", "url", "console_new", "network_new"])).optional(),
	// ...
});

// After — fixed via DiffIncludeSchema which includes "framework_state"
export const BrowserDiffParamsSchema = z.object({
	sessionId: z.string(),
	before: z.string(),
	after: z.string(),
	include: z.array(DiffIncludeSchema).optional(),
	// ...
});
```

```typescript
// Before (protocol.ts:118-127) — inline viewportConfig shape duplicated
viewportConfig: z.object({
	sourceContextLines: z.number().optional(),
	stackDepth: z.number().optional(),
	localsMaxDepth: z.number().optional(),
	localsMaxItems: z.number().optional(),
	stringTruncateLength: z.number().optional(),
	collectionPreviewItems: z.number().optional(),
}).optional(),

// After — reuse shared schema
import { ViewportConfigPartialSchema } from "../core/enums.js";
viewportConfig: ViewportConfigPartialSchema.optional(),
```

**Acceptance Criteria**:
- [ ] No inline `z.enum([...])` remains for any enum that exists in `enums.ts`
- [ ] `BrowserOverviewParamsSchema` accepts `"framework"` in `include`
- [ ] `BrowserDiffParamsSchema` accepts `"framework_state"` in `include`
- [ ] `LaunchParamsSchema.viewportConfig` uses `ViewportConfigPartialSchema`
- [ ] `AttachParamsSchema.viewportConfig` uses `ViewportConfigPartialSchema`
- [ ] All existing tests pass

---

### Unit 5: Update `src/mcp/tools/index.ts` — Import enums

**File**: `src/mcp/tools/index.ts`

**Changes**:
1. Replace inline `z.enum(["over", "into", "out"])` (line 255) with `StepDirectionSchema`
2. Replace inline `z.enum(["local", "global", "closure", "all"])` (line 397) with `VariableScopeSchema`
3. Replace inline `z.enum(["summary", "detailed"])` (line 462) with `SessionLogFormatSchema`
4. Replace inline `z.enum(["stdout", "stderr", "both"])` (line 473) with `OutputStreamSchema`

Import:
```typescript
import {
	SessionLogFormatSchema,
	StepDirectionSchema,
	OutputStreamSchema,
	VariableScopeSchema,
} from "../../core/enums.js";
```

**Note**: The local `ViewportConfigSchema` in this file uses snake_case keys with `.describe()` strings — this is the MCP-facing schema and intentionally differs from the core camelCase schema. It stays as-is because MCP tools need `.describe()` annotations and snake_case per MCP convention. The `mapViewportConfig()` bridge remains necessary.

**Acceptance Criteria**:
- [ ] No inline `z.enum([...])` for step direction, scope, format, or stream
- [ ] MCP `ViewportConfigSchema` with snake_case + `.describe()` is preserved (not replaced)
- [ ] All MCP tools continue to work

---

### Unit 6: Update `src/mcp/tools/browser.ts` — Import enums

**File**: `src/mcp/tools/browser.ts`

**Changes**:
1. Replace `z.enum(["react", "vue", "solid", "svelte"])` in chrome_start (line 84) with `FrameworkSchema`
2. Replace `z.enum(["react", "vue", "solid", "svelte"])` in session_search (line 276) with `FrameworkSchema`
3. Replace `z.enum(["timeline", ...])` in session_overview (line 207) with `OverviewIncludeSchema`
4. Replace large `z.enum([...])` in session_search event_types (lines 245-258) with `SearchableEventTypeSchema`
5. Replace `z.enum(["surrounding_events", ...])` in session_inspect (line 318) with `InspectIncludeSchema`
6. Replace `z.enum(["form_state", ...])` in session_diff (line 347) with `DiffIncludeSchema`
7. Replace `z.enum(["summary", "reproduction_steps", "test_scaffold"])` in session_replay_context (line 376) with `ReplayFormatSchema`
8. Replace `z.enum(["playwright", "cypress"])` in session_replay_context (line 378) with `TestFrameworkSchema`

Import:
```typescript
import {
	DiffIncludeSchema,
	FrameworkSchema,
	InspectIncludeSchema,
	OverviewIncludeSchema,
	ReplayFormatSchema,
	SearchableEventTypeSchema,
	TestFrameworkSchema,
} from "../../core/enums.js";
```

**Acceptance Criteria**:
- [ ] No inline `z.enum([...])` for any enum that exists in `enums.ts`
- [ ] All `.describe()` annotations on the wrapping schema fields are preserved
- [ ] All browser tools continue to work

---

### Unit 7: Update `src/browser/types.ts` — Import enums

**File**: `src/browser/types.ts`

**Changes**:
1. Remove `EventType` union (lines 1-18) — re-export from `enums.ts`
2. Replace `"react" | "vue" | "solid" | "svelte"` in `FrameworkDetectData.framework` (line 22) with `Framework`
3. Replace `"mount" | "update" | "unmount" | "store_mutation"` in `FrameworkStateData.changeType` (line 37) with `FrameworkChangeType`
4. Replace `"low" | "medium" | "high"` in `FrameworkErrorData.severity` (line 52) with `Severity`
5. Replace `"low" | "medium" | "high"` in `Marker.severity` (line 71) with `Severity`

```typescript
// Before
export type EventType = "navigation" | "network_request" | ... ;

export interface FrameworkDetectData {
	framework: "react" | "vue" | "solid" | "svelte";
	// ...
}

// After
import type { EventType, Framework, FrameworkChangeType, Severity } from "../core/enums.js";
export type { EventType };

export interface FrameworkDetectData {
	framework: Framework;
	// ...
}
```

**Acceptance Criteria**:
- [ ] `EventType` is imported and re-exported, not defined inline
- [ ] `FrameworkDetectData.framework`, `FrameworkErrorData.severity`, `Marker.severity` use enum types
- [ ] Existing imports of `EventType` from `browser/types.ts` continue to work

---

### Unit 8: Update `src/browser/recorder/framework/index.ts` — Import enums

**File**: `src/browser/recorder/framework/index.ts`

**Changes**:
1. Replace hardcoded `["react", "vue", "solid", "svelte"]` (line 28) with `FRAMEWORKS` import

```typescript
// Before
} else if (frameworkState === true) {
	this.config = { frameworks: ["react", "vue", "solid", "svelte"] };
}

// After
import { FRAMEWORKS } from "../../../core/enums.js";
// ...
} else if (frameworkState === true) {
	this.config = { frameworks: [...FRAMEWORKS] };
}
```

**Acceptance Criteria**:
- [ ] No hardcoded framework name array — uses `FRAMEWORKS` const

---

### Unit 9: Update `src/browser/recorder/auto-detect.ts` and `rolling-buffer.ts` — Import Severity

**File**: `src/browser/recorder/auto-detect.ts`

Replace all `"low" | "medium" | "high"` inline unions with `Severity` type import.

```typescript
// Before
severity: "low" | "medium" | "high";

// After
import type { Severity } from "../../../core/enums.js";
// ...
severity: Severity;
```

**File**: `src/browser/recorder/rolling-buffer.ts`

Replace `"low" | "medium" | "high"` in `placeMarker()` signature (line 28) with `Severity`.

**Acceptance Criteria**:
- [ ] No inline `"low" | "medium" | "high"` unions remain in `auto-detect.ts` or `rolling-buffer.ts`

---

### Unit 10: Update `src/browser/investigation/query-engine.ts` — Import enums

**File**: `src/browser/investigation/query-engine.ts`

**Changes**:
1. Replace `("timeline" | "markers" | "errors" | "network_summary" | "framework")[]` in `OverviewOptions.include` (line 417) with `OverviewInclude[]`
2. Replace `("surrounding_events" | ...)[]` in `InspectParams.include` (line 446) with `InspectInclude[]`

```typescript
import type { InspectInclude, OverviewInclude } from "../../core/enums.js";

export interface OverviewOptions {
	include?: OverviewInclude[];
	aroundMarker?: string;
	timeRange?: { start: number; end: number };
}

export interface InspectParams {
	eventId?: string;
	markerId?: string;
	timestamp?: number;
	include?: InspectInclude[];
	contextWindow?: number;
}
```

**Acceptance Criteria**:
- [ ] `OverviewOptions.include` and `InspectParams.include` use imported enum types
- [ ] No inline string literal arrays for include options

---

### Unit 11: Update `src/browser/investigation/replay-context.ts` — Import enums

**File**: `src/browser/investigation/replay-context.ts`

**Changes**:
1. Remove `ReplayFormat` type alias (line 4) — import from `enums.ts`
2. Remove `TestFramework` type alias (line 5) — import from `enums.ts`

```typescript
// Before
export type ReplayFormat = "summary" | "reproduction_steps" | "test_scaffold";
export type TestFramework = "playwright" | "cypress";

// After
import type { ReplayFormat, TestFramework } from "../../core/enums.js";
export type { ReplayFormat, TestFramework };
```

**Acceptance Criteria**:
- [ ] Types re-exported for backward compatibility
- [ ] No inline type definitions for replay format or test framework

---

### Unit 12: Update `src/cli/commands/index.ts` — Import enums

**File**: `src/cli/commands/index.ts`

**Changes**:
1. Replace inline `["over", "into", "out"].includes(direction)` (line 305) with `STEP_DIRECTIONS` import
2. Replace type assertion `as "over" | "into" | "out"` (line 304) with `as StepDirection`

```typescript
// Before
const direction = args.direction as "over" | "into" | "out";
if (!["over", "into", "out"].includes(direction)) {

// After
import { STEP_DIRECTIONS, type StepDirection } from "../../core/enums.js";
// ...
const direction = args.direction as StepDirection;
if (!(STEP_DIRECTIONS as readonly string[]).includes(direction)) {
```

**Acceptance Criteria**:
- [ ] No hardcoded `["over", "into", "out"]` array
- [ ] Runtime validation uses `STEP_DIRECTIONS` const

---

### Unit 13: Update `src/index.ts` — Re-export new types

**File**: `src/index.ts`

**Changes**:
Add re-exports for new enum types that consumers of the library need:

```typescript
export type { EventType, Framework, Severity, StepDirection, VariableScope } from "./core/enums.js";
```

Only export types that are part of the public API — internal enums like `OverviewInclude` stay internal.

**Acceptance Criteria**:
- [ ] Public API types from enums are accessible via `import { ... } from "agent-lens"`
- [ ] No breaking changes to existing public exports

---

## Implementation Order

1. **Unit 1**: Create `src/core/enums.ts` — no dependencies, foundation for everything else
2. **Unit 2**: Update `src/core/types.ts` — depends on Unit 1
3. **Unit 3**: Update `src/core/session-manager.ts` — depends on Unit 1
4. **Unit 4**: Update `src/daemon/protocol.ts` — depends on Unit 1 (fixes bugs)
5. **Units 5-6**: Update `src/mcp/tools/index.ts` and `browser.ts` — depends on Unit 1, parallel with each other
6. **Units 7-9**: Update `src/browser/types.ts`, `recorder/framework/index.ts`, `recorder/auto-detect.ts`, `recorder/rolling-buffer.ts` — depends on Unit 1, parallel
7. **Units 10-11**: Update `src/browser/investigation/query-engine.ts` and `replay-context.ts` — depends on Unit 1
8. **Unit 12**: Update `src/cli/commands/index.ts` — depends on Unit 1
9. **Unit 13**: Update `src/index.ts` — depends on Units 1-12

---

## Testing

### Unit Tests: `tests/unit/core/enums.test.ts`

```typescript
import { describe, expect, it } from "vitest";
import {
	DIFF_INCLUDES,
	EVENT_TYPES,
	FRAMEWORKS,
	INSPECT_INCLUDES,
	OVERVIEW_INCLUDES,
	SEARCHABLE_EVENT_TYPES,
	SESSION_STATES,
	SESSION_STATUSES,
	STEP_DIRECTIONS,
} from "../../../src/core/enums.js";

describe("enums — single source of truth", () => {
	it("SESSION_STATES is a superset of SESSION_STATUSES", () => {
		for (const status of SESSION_STATUSES) {
			expect(SESSION_STATES).toContain(status);
		}
		expect(SESSION_STATES).toContain("launching");
	});

	it("SEARCHABLE_EVENT_TYPES is a subset of EVENT_TYPES", () => {
		for (const type of SEARCHABLE_EVENT_TYPES) {
			expect(EVENT_TYPES).toContain(type);
		}
	});

	it("all include enums have non-empty tuples", () => {
		expect(OVERVIEW_INCLUDES.length).toBeGreaterThan(0);
		expect(INSPECT_INCLUDES.length).toBeGreaterThan(0);
		expect(DIFF_INCLUDES.length).toBeGreaterThan(0);
	});

	it("FRAMEWORKS contains the four supported frameworks", () => {
		expect(FRAMEWORKS).toEqual(["react", "vue", "solid", "svelte"]);
	});

	it("STEP_DIRECTIONS contains over, into, out", () => {
		expect(STEP_DIRECTIONS).toEqual(["over", "into", "out"]);
	});
});
```

### Existing Test Validation

All existing tests must continue to pass — the changes are type-level refactors + 2 bug fixes. Run:

```bash
bun run test:unit
bun run test:integration
bun run test:e2e
```

### Specific Regression Checks

- `tests/unit/core/types.test.ts` — `mapViewportConfig()` tests unaffected
- `tests/unit/core/session-logger.test.ts` — observation kinds still work with `ActionObservationKind`
- `tests/unit/mcp/tools-utils.test.ts` — `toolHandler()` wrapper unaffected
- `tests/unit/core/errors.test.ts` — error hierarchy unaffected

---

## Verification Checklist

```bash
# 1. Lint — no import errors, no unused imports
bun run lint

# 2. Type check — all types resolve
bunx tsc --noEmit

# 3. Unit tests — fast feedback
bun run test:unit

# 4. Integration tests — debugger interactions
bun run test:integration

# 5. E2E tests — full MCP/CLI/browser flows
bun run test:e2e

# 6. Grep for remaining inline enum duplication
rg '"over".*"into".*"out"' src/ --glob '!enums.ts'
rg '"react".*"vue".*"solid".*"svelte"' src/ --glob '!enums.ts'
rg '"low".*"medium".*"high"' src/ --glob '!enums.ts'
rg '"summary".*"detailed"' src/ --glob '!enums.ts' --glob '!session-logger.ts'
```

The final grep commands should return zero matches (except in `.describe()` documentation strings, which are prose, not type definitions).
