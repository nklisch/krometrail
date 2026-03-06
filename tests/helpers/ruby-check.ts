import { spawn } from "node:child_process";

/**
 * Check if rdbg (Ruby debug gem) is available.
 */
export async function isRdbgAvailable(): Promise<boolean> {
	return new Promise((resolve) => {
		const proc = spawn("rdbg", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
}

/**
 * Whether rdbg is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_RDBG: boolean = await isRdbgAvailable().then((ok) => !ok);
