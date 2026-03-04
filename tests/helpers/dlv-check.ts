import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";

function dlvEnv(): NodeJS.ProcessEnv {
	const goBin = process.env.GOPATH ? join(process.env.GOPATH, "bin") : join(homedir(), "go", "bin");
	const currentPath = process.env.PATH ?? "";
	const augmentedPath = currentPath.includes(goBin) ? currentPath : `${goBin}:${currentPath}`;
	return { ...process.env, PATH: augmentedPath };
}

/**
 * Check if Delve (dlv) is installed and usable.
 */
export async function isDlvAvailable(): Promise<boolean> {
	return new Promise((resolve) => {
		const proc = spawn("dlv", ["version"], { stdio: "pipe", env: dlvEnv() });
		proc.on("close", (code) => {
			resolve(code === 0);
		});
		proc.on("error", () => resolve(false));
	});
}

/**
 * Whether dlv is available for the current test run.
 * Computed once at module load time for use with describe.skipIf.
 */
export const SKIP_NO_DLV: boolean = await isDlvAvailable().then((ok) => !ok);
