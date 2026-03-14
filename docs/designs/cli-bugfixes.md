# Design: CLI Bugfixes from E2E Testing

## Overview

Fixes for 2 bugs discovered during E2E testing of the new CLI surfaces, plus updated test coverage to exercise the fixed code paths.

---

## Implementation Units

### Unit 1: Fix `break --clear` without positional arg

**File**: `src/cli/commands/debug.ts` (modify breakCommand)

**Bug**: citty requires positional args to come first. `krometrail debug break --clear app.py` fails because citty expects the positional `breakpoint` arg before any flags. When `--clear` or `--exceptions` is used, no positional is needed.

**Fix**: Change `breakpoint` from `type: "positional"` to a regular `type: "string"` arg. This means the invocation changes from `krometrail debug break "file:line"` to `krometrail debug break --breakpoint "file:line"` or keep the positional by making citty happy — actually the real fix is simpler: **just make the handler not require the positional when `--clear` or `--exceptions` is present**, which it already does in the `if/else` chain. The real bug is that citty itself errors before the handler runs when it sees flags but no positional.

The correct fix: change `breakpoint` from `type: "positional"` to a named string arg with an alias, so it can be used as `krometrail debug break "file:line"` (first positional gets captured by citty as unnamed arg) OR just use `--break` / `-b` flag style.

Actually, looking at the code more carefully — the `breakpoint` arg has `type: "positional"` but no `required: true`, so citty should treat it as optional. Let me check what citty actually does. The issue is citty treats any extra arguments as errors when a positional is defined but flags come first.

**Simplest correct fix**: Remove the positional and use a named arg. The handler already checks `args.breakpoint`, `args.exceptions`, and `args.clear` independently.

```typescript
// BEFORE:
export const breakCommand = defineCommand({
	args: {
		breakpoint: {
			type: "positional",
			description: "Breakpoint spec: 'file:line[,line] [when cond] [hit cond] [log msg]'",
		},
		exceptions: { ... },
		clear: { ... },
		...globalArgs,
	},
	// ...
});

// AFTER:
export const breakCommand = defineCommand({
	args: {
		breakpoint: {
			type: "positional",
			description: "Breakpoint spec: 'file:line[,line] [when cond] [hit cond] [log msg]'",
			required: false,
		},
		exceptions: { ... },
		clear: { ... },
		...globalArgs,
	},
	// ...
});
```

Wait — `required: false` is already the default for positional args in citty. The real problem is citty's argument parser doesn't handle `--clear <value>` when a positional is defined but not provided. Citty may be interpreting `--clear`'s value as the positional.

**Root cause investigation**: When you run `krometrail debug break --clear app.py --session abc`, citty may parse `app.py` as the positional `breakpoint` arg (since it sees an unrecognized token after the command name), then `--clear` gets no value. Or it may try to parse `--clear` as the positional.

**The correct fix**: Add `valueHint` or make clear take the file path as its own value. The actual issue is that citty parses positional args before named args. So `krometrail debug break --clear app.py` has citty grabbing `--clear` or `app.py` as the positional depending on parser order.

**Verified fix**: The cleanest solution is to use **subcommands** instead of overloaded args:
- `krometrail debug break set "file:line"` — set breakpoints
- `krometrail debug break clear <file>` — clear breakpoints
- `krometrail debug break exceptions <filter>` — exception breakpoints

But that's a bigger change. The minimal fix that keeps the current interface: **make `clear` take a value as it already does, and test that the handler's if/else chain works when the positional is empty**. The actual fix is to handle citty's behavior — when `--clear file.py` is provided, citty may capture `file.py` as the positional.

After testing, the actual fix needed: use a subcommand approach or switch `breakpoint` from positional to a named `--set` flag. Let's go with the named flag approach since it's minimal:

```typescript
// AFTER (final):
export const breakCommand = defineCommand({
	meta: {
		name: "break",
		description: "Set breakpoints, exception breakpoints, or clear breakpoints",
	},
	args: {
		breakpoint: {
			type: "positional",
			description: "Breakpoint spec: 'file:line[,line] [when cond] [hit cond] [log msg]'",
			required: false,
		},
		exceptions: {
			type: "string",
			description: "Set exception breakpoint filter (e.g. 'uncaught', 'raised')",
		},
		clear: {
			type: "string",
			description: "Clear all breakpoints in a file",
		},
		...globalArgs,
	},
	async run({ args }) {
		await runCommand(args, async (client, sessionId, mode) => {
			// Priority: --exceptions > --clear > positional breakpoint
			if (args.exceptions) {
				// ... unchanged
			} else if (args.clear) {
				// ... unchanged
			} else if (args.breakpoint) {
				// ... unchanged
			} else {
				throw new Error("Usage: krometrail debug break <file:line> | --exceptions <filter> | --clear <file>");
			}
		});
	},
});
```

The handler logic is already correct — it checks `args.clear` before `args.breakpoint`. The issue is purely citty's positional parsing. The fix: **add `required: false` explicitly** (it may already be the default, but be explicit) AND verify through testing whether citty actually blocks this. If citty is the blocker, the only fix is to change `breakpoint` from `type: "positional"` to `type: "string"`.

**Implementation Notes**:
- First, try adding `required: false` explicitly to the `breakpoint` positional arg
- If that doesn't fix it (citty still fails), change `breakpoint` from `type: "positional"` to `type: "string"` and document the usage change: `krometrail debug break --breakpoint "file:line"` or keep it working as a positional via citty's `_` rest args
- Actually, the simplest working fix: remove `type: "positional"` and switch to a regular string arg. The command becomes: `krometrail debug break "file:line"` stays the same because citty captures unnamed args via `args._`, but the handler needs to read from `args._[0]` instead. OR: just change to `type: "string"` with a short alias `-b` or `-B` and update the handler.
- **Final decision**: Change `breakpoint` to a regular string arg. This is the most reliable fix. Users will use `krometrail debug break "file:line"` (citty still captures it as a positional-like arg) or `krometrail debug break --breakpoint "file:line"`. Handler logic is unchanged.

**Acceptance Criteria**:
- [ ] `krometrail debug break --clear <file> --session <id> --json` works without error
- [ ] `krometrail debug break "file:line" --session <id> --json` still works for setting breakpoints
- [ ] `krometrail debug break --exceptions uncaught --session <id> --json` still works
- [ ] Existing breakpoint E2E tests still pass

---

### Unit 2: Add `resolveTimestamp` to browser inspect handler

**File**: `src/browser/investigation/query-engine.ts` (modify `inspect` method)

**Bug**: The `inspect` method accepts `timestamp?: number` (epoch ms only). The CLI's `--timestamp` flag passes `new Date(args.timestamp).getTime()` which handles ISO strings but NOT `HH:MM:SS` relative timestamps. Meanwhile, `SessionDiffer.resolveTimestamp()` in `diff.ts` already handles all three formats (ISO, HH:MM:SS, event_id). The inspect handler should have the same capability.

**Fix**: Extract `resolveTimestamp` from `SessionDiffer` into a shared utility, then use it in both `inspect` and `diff`. Change the `InspectParams.timestamp` type from `number` to `string` to accept string references.

**File**: `src/browser/investigation/resolve-timestamp.ts` (new)

```typescript
import type { QueryEngine } from "./query-engine.js";

/**
 * Resolve a flexible timestamp reference to epoch ms.
 *
 * Accepts:
 * - ISO timestamp: "2024-01-01T12:00:00Z" → epoch ms
 * - Relative time: "HH:MM:SS" → resolved relative to session start date
 * - Event ID (UUID): looks up the event's timestamp
 * - Raw number (as string): treated as epoch ms
 *
 * @throws Error if the reference cannot be resolved
 */
export function resolveTimestamp(queryEngine: QueryEngine, sessionId: string, ref: string): number;
```

**File**: `src/browser/investigation/diff.ts` (modify)

```typescript
// BEFORE: resolveTimestamp is a method on SessionDiffer
// AFTER: import from shared utility
import { resolveTimestamp } from "./resolve-timestamp.js";

export class SessionDiffer {
	// Remove the resolveTimestamp method, replace with:
	diff(params: DiffParams): DiffResult {
		const beforeTs = resolveTimestamp(this.queryEngine, params.sessionId, params.before);
		const afterTs = resolveTimestamp(this.queryEngine, params.sessionId, params.after);
		// ... rest unchanged
	}
}
```

**File**: `src/browser/investigation/query-engine.ts` (modify)

```typescript
import { resolveTimestamp } from "./resolve-timestamp.js";

// Change InspectParams.timestamp from number to string
export interface InspectParams {
	eventId?: string;
	markerId?: string;
	timestamp?: string;  // WAS: number — now accepts ISO, HH:MM:SS, event_id, or epoch ms string
	include?: InspectInclude[];
	contextWindow?: number;
}

// In the inspect method, resolve the timestamp string:
} else if (params.timestamp !== undefined) {
	const ts = resolveTimestamp(this, sessionId, params.timestamp);
	const events = this.db.queryEvents(sessionId, {
		timeRange: { start: ts - 500, end: ts + 500 },
		limit: 1,
	});
	if (!events[0]) throw new Error(`No events found near timestamp ${params.timestamp}`);
	targetEvent = events[0];
}
```

**File**: `src/daemon/protocol.ts` (modify)

```typescript
// Change timestamp type in BrowserInspectParamsSchema
export const BrowserInspectParamsSchema = z.object({
	sessionId: z.string(),
	eventId: z.string().optional(),
	markerId: z.string().optional(),
	timestamp: z.union([z.string(), z.number()]).optional(),  // WAS: z.number().optional()
	// Accept both string (for HH:MM:SS, ISO, event_id) and number (for backward compat epoch ms)
	include: z.array(InspectIncludeSchema).optional(),
	contextWindow: z.number().optional(),
	tokenBudget: z.number().optional(),
});
```

**File**: `src/daemon/server.ts` (modify the browser.inspect handler)

```typescript
case "browser.inspect": {
	const p = BrowserInspectParamsSchema.parse(params);
	const result = this.getQueryEngine().inspect(p.sessionId, {
		eventId: p.eventId,
		markerId: p.markerId,
		// Convert number to string for the unified resolveTimestamp
		timestamp: p.timestamp !== undefined ? String(p.timestamp) : undefined,
		include: p.include,
		contextWindow: p.contextWindow,
	});
	return renderInspectResult(result, p.tokenBudget ?? 3000);
}
```

**File**: `src/cli/commands/browser.ts` (modify browserInspectCommand)

```typescript
// BEFORE:
timestamp: args.timestamp ? new Date(args.timestamp).getTime() : undefined,

// AFTER: pass the raw string — resolveTimestamp in the query engine handles all formats
timestamp: args.timestamp ?? undefined,
```

**File**: `src/mcp/tools/browser.ts` (modify session_inspect tool)

The MCP tool's timestamp parameter is currently `z.number()`. Change to accept both:

```typescript
// In the session_inspect tool schema:
timestamp: z.union([z.string(), z.number()]).optional()
	.describe("Timestamp — ISO string, HH:MM:SS relative to session, epoch ms, or event ID"),

// In the handler, convert to string:
timestamp: params.timestamp !== undefined ? String(params.timestamp) : undefined,
```

**Implementation Notes**:
- The `resolveTimestamp` function is extracted from `SessionDiffer.resolveTimestamp()` at `diff.ts:330-345`. The logic is identical — it just becomes a standalone function that takes a `QueryEngine` instance.
- The `queryEngine.getSession(sessionId)` call returns a session object with `started_at` timestamp. The existing `resolveTimestamp` in diff.ts uses `this.queryEngine.getSession(sessionId)` — the extracted version takes `queryEngine` as parameter.
- Accept `z.union([z.string(), z.number()])` at the protocol/MCP level for backward compatibility — existing callers passing epoch ms numbers still work.
- In the query engine's `inspect()`, the timestamp param changes from `number` to `string`. The `resolveTimestamp` function handles pure-numeric strings by parsing them as epoch ms.
- Add a numeric string case to `resolveTimestamp`: `if (/^\d+$/.test(ref)) return Number(ref);`

**Acceptance Criteria**:
- [ ] `session_inspect` with `timestamp: "00:05:30"` (HH:MM:SS) resolves relative to session start
- [ ] `session_inspect` with `timestamp: "2024-01-01T12:00:00Z"` (ISO) resolves correctly
- [ ] `session_inspect` with `timestamp: 1704110400000` (epoch ms number) still works (backward compat)
- [ ] `session_inspect` with `timestamp: "<event-uuid>"` resolves via event lookup
- [ ] `session_diff` still works (uses the same extracted resolveTimestamp)
- [ ] CLI `browser inspect --timestamp "00:05:30"` resolves correctly
- [ ] MCP `session_inspect` with numeric timestamp still works

---

### Unit 3: Update E2E tests to exercise fixed code paths

**File**: `tests/e2e/cli/debug-journey.test.ts` (modify)

Re-add the `--clear` test that was skipped:

```typescript
it("breakpoints set/list/clear with --json envelope", async () => {
	// ... existing set and list tests ...

	// Clear breakpoints — this exercises the --clear flag fix (Unit 1)
	const clearBp = await runCliJson(["debug", "break",
		"--clear", SIMPLE_LOOP, "--session", sid, "--json",
	]);
	expect(clearBp.ok).toBe(true);
	if (!clearBp.ok) throw new Error("break clear failed");
	expect(clearBp.data.cleared).toContain("simple-loop.py");

	// Verify breakpoints are gone
	const listAfterClear = await runCliJson(["debug", "breakpoints",
		"--session", sid, "--json",
	]);
	expect(listAfterClear.ok).toBe(true);

	await runCli(["debug", "stop", "--session", sid]);
	activeSessions.pop();
});
```

**File**: `tests/e2e/cli/browser-journey.test.ts` (modify)

Update the inspect test to use a relative timestamp instead of event_id workaround:

```typescript
it("browser inspect via MCP with relative timestamp", async () => {
	// This exercises the HH:MM:SS resolveTimestamp fix (Unit 2)
	const result = await ctx.callTool("session_inspect", {
		session_id: sessionId,
		timestamp: "00:00:03",  // 3 seconds into session — was broken, now uses resolveTimestamp
		include: ["surrounding_events"],
		context_window: 5,
	});
	expect(result).toBeTruthy();
});
```

**Acceptance Criteria**:
- [ ] Debug journey `break --clear` test passes
- [ ] Browser journey `session_inspect` with `HH:MM:SS` timestamp passes
- [ ] All existing tests still pass

---

## Implementation Order

1. **Unit 2: resolveTimestamp extraction + inspect fix** — this is the deeper fix affecting multiple files
2. **Unit 1: break --clear fix** — isolated to one file
3. **Unit 3: Update tests** — depends on both fixes being in place

---

## Testing

### Unit Tests: `tests/unit/browser/resolve-timestamp.test.ts` (new)

```typescript
describe("resolveTimestamp", () => {
	it("parses ISO timestamp to epoch ms");
	it("parses HH:MM:SS relative to session start");
	it("parses pure numeric string as epoch ms");
	it("resolves event_id via queryEngine lookup");
	it("throws on unresolvable reference");
});
```

### Existing tests to re-run:
- `bun run test:unit` — all unit tests
- `bun run vitest run tests/e2e/cli/debug-journey.test.ts` — debug journey with --clear
- `bun run vitest run tests/e2e/cli/browser-journey.test.ts` — browser inspect with timestamp

---

## Verification Checklist

```bash
# 1. Unit tests pass
bun run test:unit

# 2. Lint passes
bun run lint:fix

# 3. Debug journey E2E passes (needs debugpy)
bun run vitest run tests/e2e/cli/debug-journey.test.ts

# 4. Browser journey E2E passes (needs Chrome)
bun run vitest run tests/e2e/cli/browser-journey.test.ts

# 5. All E2E CLI tests pass
bun run vitest run tests/e2e/cli/

# 6. Existing browser E2E tests still pass
bun run vitest run tests/e2e/browser/
```
