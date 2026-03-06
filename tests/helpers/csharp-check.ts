import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";

/**
 * Check if dotnet CLI and netcoredbg are available.
 * netcoredbg may be on PATH or in the agent-lens cache.
 */
export async function isNetcoredbgAvailable(): Promise<boolean> {
	const dotnetOk = await new Promise<boolean>((resolve) => {
		const proc = spawn("dotnet", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (!dotnetOk) return false;

	// Check PATH
	const onPath = await new Promise<boolean>((resolve) => {
		const proc = spawn("netcoredbg", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (onPath) return true;

	// Check cache
	const ext = platform() === "win32" ? ".exe" : "";
	const cached = join(homedir(), ".agent-lens", "adapters", "netcoredbg", `netcoredbg${ext}`);
	return existsSync(cached);
}

/**
 * Whether C# debugging is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_CSHARP: boolean = await isNetcoredbgAvailable().then((ok) => !ok);
