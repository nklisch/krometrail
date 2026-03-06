import { exec, spawn } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

/**
 * Check if Swift compiler and lldb-dap are both available.
 */
export async function isSwiftDebugAvailable(): Promise<boolean> {
	const swiftOk = await new Promise<boolean>((resolve) => {
		const proc = spawn("swiftc", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (!swiftOk) return false;

	// Check lldb-dap on PATH
	const lldbOk = await new Promise<boolean>((resolve) => {
		const proc = spawn("lldb-dap", ["--version"], { stdio: "pipe" });
		proc.on("close", (code) => resolve(code === 0));
		proc.on("error", () => resolve(false));
	});
	if (lldbOk) return true;

	// macOS fallback: xcrun -f lldb-dap
	if (process.platform === "darwin") {
		try {
			await execAsync("xcrun -f lldb-dap");
			return true;
		} catch {
			return false;
		}
	}

	return false;
}

/**
 * Whether Swift debugging is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_SWIFT: boolean = await isSwiftDebugAvailable().then((ok) => !ok);
