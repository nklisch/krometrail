# Pattern: Shared Adapter Utilities Module

All language adapters share a set of utility functions in `src/adapters/helpers.ts` for common operations: checking prerequisites, allocating ports, spawning processes, TCP connections, and cleanup. Adapters import from helpers rather than duplicating logic.

## Rationale
10 language adapters need the same low-level operations. Centralizing them in helpers.ts prevents copy-paste divergence and provides tested, well-typed implementations.

## Examples

### Example 1: Prerequisite Checking
**File**: `src/adapters/helpers.ts:19-34`
```typescript
export function checkCommand(opts: {
	cmd: string;
	args: string[];
	env?: NodeJS.ProcessEnv;
	missing: string[];
	installHint: string;
}): Promise<PrerequisiteResult> {
	return new Promise((resolve) => {
		const spawnEnv = opts.env !== undefined ? { ...process.env, ...opts.env } : undefined;
		const proc = spawn(opts.cmd, opts.args, { stdio: "pipe", env: spawnEnv });
		const fail = (): void => resolve({ satisfied: false, missing: opts.missing, installHint: opts.installHint });
		proc.on("close", (code) => (code === 0 ? resolve({ satisfied: true }) : fail()));
		proc.on("error", fail);
	});
}
```

### Example 2: Process Spawning with Readiness Detection
**File**: `src/adapters/helpers.ts:84-129`
```typescript
export function spawnAndWait(options: SpawnAndWaitOptions): Promise<SpawnResult> {
	const { cmd, args, cwd, env, readyPattern, timeoutMs = 10_000, label } = options;
	return new Promise((resolve, reject) => {
		const child = spawn(cmd, args, { cwd, env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"] });
		const timeout = setTimeout(() => {
			child.kill();
			reject(new LaunchError(`${label} did not start within ${timeoutMs}ms`));
		}, timeoutMs);
		const onData = (data: Buffer) => {
			if (readyPattern.test(data.toString())) {
				clearTimeout(timeout);
				resolve({ process: child, stderrBuffer: outputChunks.join("") });
			}
		};
		child.stdout?.on("data", onData);
		child.stderr?.on("data", onData);
	});
}
```

### Example 3: Graceful Process Cleanup
**File**: `src/adapters/helpers.ts:159-176`
```typescript
export async function gracefulDispose(socket: Socket | null, proc: ChildProcess | null): Promise<void> {
	if (socket) socket.destroy();
	if (proc) {
		proc.kill("SIGTERM");
		await new Promise<void>((resolve) => {
			const timeout = setTimeout(() => { proc.kill("SIGKILL"); resolve(); }, 2_000);
			proc.once("close", () => { clearTimeout(timeout); resolve(); });
		});
	}
}
```

## When to Use
- Any new language adapter's `checkPrerequisites()` — use `checkCommand()`
- Any adapter that spawns a process — use `spawnAndWait()` or `detectEarlySpawnFailure()`
- Any adapter that needs a TCP port — use `allocatePort()`
- Any adapter `dispose()` — use `gracefulDispose()`
- Any adapter that downloads tools — use `downloadToFile()`, `getAdapterCacheDir()`, `ensureAdapterCacheDir()`

## When NOT to Use
- Code outside `src/adapters/` — these helpers are adapter-specific
- When the helper's timeout/retry behavior doesn't fit — consider adding a variant to helpers.ts rather than duplicating inline

## Common Violations
- Calling `spawn()` directly in an adapter instead of using `spawnAndWait()` — misses timeout, error, and early-exit handling
- Implementing custom SIGTERM→SIGKILL logic in an adapter instead of using `gracefulDispose()`
- Using hardcoded port numbers instead of `allocatePort()`
