import { spawn } from "node:child_process";

/**
 * Check if Node.js 18+ is available.
 */
export async function isNodeDebugAvailable(): Promise<boolean> {
	return new Promise((resolve) => {
		const proc = spawn("node", ["--version"], { stdio: "pipe" });
		let output = "";
		proc.stdout?.on("data", (d: Buffer) => {
			output += d.toString();
		});
		proc.on("close", (code) => {
			if (code !== 0) {
				resolve(false);
				return;
			}
			const match = output.trim().match(/^v(\d+)/);
			const major = match ? parseInt(match[1], 10) : 0;
			resolve(major >= 18);
		});
		proc.on("error", () => resolve(false));
	});
}

/**
 * Whether Node debug is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_NODE_DEBUG: boolean = await isNodeDebugAvailable().then((ok) => !ok);
