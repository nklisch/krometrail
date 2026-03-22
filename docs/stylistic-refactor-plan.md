# Stylistic Refactor Plan

Generated 2026-03-21. **All High Value items implemented.**

**Styles with zero violations:** Arrow vs Declaration, Explicit Return Types, Classes vs Functions, Composition over Inheritance.

---

## High Value

Refactors that significantly improve readability with low risk.

---

#### 1. Rename abbreviated variables in `src/cli/commands/doctor.ts`

**File:** `src/cli/commands/doctor.ts` (lines 112, 141, 169, 192, 215, 241, 264, 299, 323, 349)
**Style:** Descriptive Naming

`proc` appears 10 times across diagnostic check functions. Each is a short-lived
`Bun.spawn` result.

**Current:**
```typescript
const proc = Bun.spawn(["python3", "--version"], { stdout: "pipe", stderr: "pipe" });
```

**Target:**
```typescript
const process = Bun.spawn(["python3", "--version"], { stdout: "pipe", stderr: "pipe" });
```

> Note: If `process` conflicts with the global, use `spawnResult` or `child` instead.

**Acceptance Criteria:**
- [ ] All 10 `proc` instances renamed consistently
- [ ] No shadowing of `globalThis.process`
- [ ] Tests pass
- [ ] No behavior change

---

#### 2. Rename abbreviated variables in `src/browser/executor/cdp-adapter.ts`

**File:** `src/browser/executor/cdp-adapter.ts` (lines 42, 48, 337, 343, 350, 356)
**Style:** Descriptive Naming

`res` (6 instances) and `msg` (3 instances) used for CDP responses and error messages.

**Current:**
```typescript
const res = await this.send("Runtime.evaluate", { expression, ... });
if (!res.result) {
	const msg = res.exceptionDetails?.exception?.description ?? "Unknown error";
```

**Target:**
```typescript
const response = await this.send("Runtime.evaluate", { expression, ... });
if (!response.result) {
	const errorMessage = response.exceptionDetails?.exception?.description ?? "Unknown error";
```

**Acceptance Criteria:**
- [ ] All `res` → `response`, `msg` → `errorMessage`
- [ ] Tests pass
- [ ] No behavior change

---

#### 3. Rename abbreviated variables in `src/browser/recorder/cdp-client.ts`

**File:** `src/browser/recorder/cdp-client.ts` (lines 25, 134)
**Style:** Descriptive Naming

**Current:**
```typescript
const resp = await fetch(`http://127.0.0.1:${port}/json/list`);
// ...
const msg = JSON.parse(data.toString());
```

**Target:**
```typescript
const response = await fetch(`http://127.0.0.1:${port}/json/list`);
// ...
const message = JSON.parse(data.toString());
```

**Acceptance Criteria:**
- [ ] `resp` → `response`, `msg` → `message`
- [ ] Tests pass
- [ ] No behavior change

---

#### 4. Rename `desc` in `src/cli/commands/completions.ts` and `src/cli/format.ts`

**File:** `src/cli/commands/completions.ts` (lines 115, 273, 288)
**File:** `src/cli/format.ts` (line 254)
**Style:** Descriptive Naming

**Current:**
```typescript
const desc = command.description ?? "";
```

**Target:**
```typescript
const description = command.description ?? "";
```

**Acceptance Criteria:**
- [ ] All `desc` → `description` in both files
- [ ] Tests pass
- [ ] No behavior change

---

#### 5. Rename `val` and `prev` in Vue injection/patterns

**File:** `src/browser/recorder/framework/vue-injection.ts` (lines 297, 404)
**File:** `src/browser/recorder/framework/patterns/vue-patterns.ts` (line 57)
**Style:** Descriptive Naming

**Current:**
```typescript
const val = component.data[key];
const prev = oldValues.get(key);
```

**Target:**
```typescript
const value = component.data[key];
const previous = oldValues.get(key);
```

**Acceptance Criteria:**
- [ ] `val` → `value`, `prev` → `previous`
- [ ] Tests pass
- [ ] No behavior change

---

#### 6. Rename `res` and `buf` in `src/core/auto-update.ts`

**File:** `src/core/auto-update.ts` (lines 107, 180, 183)
**Style:** Descriptive Naming

**Current:**
```typescript
const res = await fetch(url);
const buf = await res.arrayBuffer();
```

**Target:**
```typescript
const response = await fetch(url);
const buffer = await response.arrayBuffer();
```

**Acceptance Criteria:**
- [ ] `res` → `response`, `buf` → `buffer`
- [ ] Tests pass
- [ ] No behavior change

---

#### 7. Rename `buf` in `src/adapters/js-debug-adapter.ts`

**File:** `src/adapters/js-debug-adapter.ts` (line 137)
**Style:** Descriptive Naming

**Current:**
```typescript
const buf = await Bun.file(cachedVersionFile).text();
```

**Target:**
```typescript
const cachedVersion = await Bun.file(cachedVersionFile).text();
```

**Acceptance Criteria:**
- [ ] `buf` → `cachedVersion` (more descriptive than just `buffer`)
- [ ] Tests pass
- [ ] No behavior change

---

#### 8. Rename `ctx` in `src/browser/recorder/framework/react-injection.ts`

**File:** `src/browser/recorder/framework/react-injection.ts` (line 206)
**Style:** Descriptive Naming

**Current:**
```typescript
const ctx = fiber._debugContext;
```

**Target:**
```typescript
const context = fiber._debugContext;
```

**Acceptance Criteria:**
- [ ] `ctx` → `context`
- [ ] Tests pass
- [ ] No behavior change

---

#### 9. Rename `msg` in `src/mcp/tools/browser.ts`

**File:** `src/mcp/tools/browser.ts` (line 117)
**Style:** Descriptive Naming

**Current:**
```typescript
const msg = e instanceof Error ? e.message : String(e);
```

**Target:**
```typescript
const errorMessage = e instanceof Error ? e.message : String(e);
```

**Acceptance Criteria:**
- [ ] `msg` → `errorMessage`
- [ ] Tests pass
- [ ] No behavior change

---

#### 10. Rename `val` in `src/telemetry.ts`

**File:** `src/telemetry.ts` (line 17)
**Style:** Descriptive Naming

**Current:**
```typescript
const val = process.env.KROMETRAIL_TELEMETRY;
```

**Target:**
```typescript
const telemetryEnv = process.env.KROMETRAIL_TELEMETRY;
```

**Acceptance Criteria:**
- [ ] `val` → `telemetryEnv`
- [ ] Tests pass
- [ ] No behavior change

---

#### 11. Early return refactor in `src/cli/commands/debug.ts`

**File:** `src/cli/commands/debug.ts` (lines 158-228)
**Style:** Early Returns

The launch handler wraps the entire body in `if (args.config || args["config-name"]) { ... } else { ... }`.
The else branch is the simpler case and should come first as a guard.

**Current:**
```typescript
if (args.config || args["config-name"]) {
	// 50+ lines of config-based launch
} else {
	if (!args.command) {
		throw new Error('Usage: ...');
	}
	const result = await client.call("session.launch", { ... });
	process.stdout.write(`${formatLaunch(result, mode)}\n`);
}
```

**Target:**
```typescript
if (!args.config && !args["config-name"]) {
	if (!args.command) {
		throw new Error('Usage: ...');
	}
	const result = await client.call("session.launch", { ... });
	process.stdout.write(`${formatLaunch(result, mode)}\n`);
	return;
}

// Config-based launch (main flow, no else wrapper)
const configPath = args.config ? resolvePath(args.config) : resolvePath(process.cwd(), ".vscode/launch.json");
// ...
```

**Acceptance Criteria:**
- [ ] Outer if/else eliminated via early return
- [ ] Inner config-name lookup also uses guard clause
- [ ] Tests pass
- [ ] No behavior change

---

#### 12. Add `is` prefix to boolean in `src/core/compression.ts`

**File:** `src/core/compression.ts` (line 9)
**Style:** Descriptive Naming

**Current:**
```typescript
active: boolean;
```

**Target:**
```typescript
isActive: boolean;
```

> Note: This is a field in the `CompressionTier` type — check all usage sites.

**Acceptance Criteria:**
- [ ] Field renamed in type definition
- [ ] All references updated
- [ ] Tests pass
- [ ] No behavior change

---

## Worth Considering

Valid refactors with moderate impact or moderate effort.

---

- **`src/adapters/helpers.ts:28,57`** — `proc` → `childProcess`. Used in `spawnAndWait` and `gracefulDispose`. Low risk but touches shared adapter code.

- **`src/browser/recorder/chrome-launcher.ts:34`** — `proc` → `chromeProcess`. Single instance, simple rename.

- **`src/daemon/client.ts:135-142`** — Invert `if (!Number.isNaN(pid))` to a guard clause `if (Number.isNaN(pid))` with early continuation. Minor readability improvement.

- **`src/core/auto-update.ts:263-273`** — Refactor else-if chain to use early returns for each install type case. Moderate improvement.

---

## Not Worth It

Code that technically violates a style but should NOT be refactored.

---

- **`src/mcp/tools/index.ts` (464 lines) and `src/mcp/tools/browser.ts` (359 lines)** — Function Size. These are tool registration functions that define schemas and handlers in sequence. Splitting into individual files per tool would add file sprawl without improving readability — the sequential registration is the natural structure.

- **`src/browser/recorder/framework/*-injection.ts` (200-412 lines each)** — Function Size. These functions generate JavaScript strings for browser injection. They're essentially template literals, not complex logic. Splitting them would fragment the injection scripts and make them harder to reason about as a whole.

- **`src/browser/recorder/framework/detector.ts` (300 lines)** — Function Size. Detection script generator — same justification as injection scripts.

- **`src/browser/recorder/marker-overlay.ts` (118 lines)** — Function Size. UI overlay script generator. Splitting would obscure the HTML/CSS/JS structure.

- **`src/cli/commands/completions.ts` (78-114 lines per function)** — Function Size. Shell completion script generators (bash, zsh, fish). These are string templates by nature.

- **`src/browser/investigation/renderers.ts` (85-117 lines per function)** — Function Size. Render functions build multi-section text output. Splitting into sub-renderers would add indirection without improving clarity — the sections are tightly coupled.

- **`src/core/viewport.ts:buildViewport` (66 lines)** — Function Size. Just over the soft limit, builds a single viewport string with source context, variables, and stack frames. Extracting sections would fragment the output contract.

- **`src/core/value-renderer.ts:renderValue` (65 lines)** — Function Size. Recursive value renderer with a switch over value types. Each case is 3-5 lines — splitting adds function call overhead without readability gain.

- **`src/core/launch-json.ts:configToOptions` (77 lines)** — Function Size. Mapping from VS Code launch.json format to internal options. The mapping is procedural by nature — each field is a 1-2 line conversion.

- **`src/adapters/python.ts:87-95`** — Early Returns. The if/else-if/else chain for script type (`-m`, `-c`, default) has three equally weighted branches. Not a guard clause candidate — it's a legitimate triple branch. A switch would be an option but the current style is clear enough.

- **`src/browser/recorder/index.ts:262-268`** — Early Returns. The nested if/else for expectedUrl handling has two meaningful branches that return differently. Flattening into a compound condition would reduce readability.
