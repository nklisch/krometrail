import { spawn } from "node:child_process";

/**
 * Check if kotlinc and JDK 17+ are both available.
 */
export async function isKotlinDebugAvailable(): Promise<boolean> {
	const kotlinOk = await new Promise<boolean>((resolve) => {
		const proc = spawn("kotlinc", ["-version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (!kotlinOk) return false;

	// Check JDK 17+
	const jdkOk = await new Promise<boolean>((resolve) => {
		const proc = spawn("javac", ["-version"], { stdio: "pipe" });
		let output = "";
		proc.stdout?.on("data", (d: Buffer) => {
			output += d.toString();
		});
		proc.stderr?.on("data", (d: Buffer) => {
			output += d.toString();
		});
		proc.on("close", (code) => {
			if (code !== 0) {
				resolve(false);
				return;
			}
			const match = output.match(/javac\s+(\d+)/);
			const major = match ? parseInt(match[1], 10) : 0;
			resolve(major >= 17);
		});
		proc.on("error", () => resolve(false));
	});
	return jdkOk;
}

/**
 * Whether Kotlin debugging is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_KOTLIN: boolean = await isKotlinDebugAvailable().then((ok) => !ok);
