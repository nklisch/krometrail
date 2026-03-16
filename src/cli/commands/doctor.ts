import { resolve as resolvePath } from "node:path";
import { defineCommand } from "citty";
import { listAdapters, registerAllAdapters } from "../../adapters/registry.js";
import { listConfigurations, parseLaunchJson } from "../../core/launch-json.js";
import { listDetectors, registerAllDetectors } from "../../frameworks/index.js";
import { successEnvelope } from "../envelope.js";
import type { OutputMode } from "../format.js";
import { resolveOutputMode } from "../format.js";

export interface DoctorResult {
	platform: string;
	runtime: string;
	runtimeVersion: string;
	adapters: Array<{
		id: string;
		displayName: string;
		status: "available" | "missing";
		version?: string;
		installHint?: string;
		fixCommand?: string;
	}>;
	frameworks: Array<{
		id: string;
		displayName: string;
		adapterId: string;
	}>;
	launchConfigs?: Array<{
		name: string;
		type: string;
		request: string;
	}>;
}

/**
 * Run all doctor checks and return structured results.
 */
export async function runDoctorChecks(): Promise<DoctorResult> {
	const platform = `${process.platform} ${process.arch}`;

	// Detect runtime version
	const bunVersion = (typeof Bun !== "undefined" ? Bun.version : null) ?? process.versions.bun ?? process.version;
	const runtimeName = process.versions.bun ? "Bun" : "Node.js";
	const runtime = runtimeName;
	const runtimeVersion = bunVersion;

	const adapters = listAdapters();
	const adapterResults: DoctorResult["adapters"] = [];

	for (const adapter of adapters) {
		const prereq = await adapter.checkPrerequisites();
		if (prereq.satisfied) {
			let version: string | undefined;
			if (adapter.id === "python") {
				version = await getPythonDebugpyVersion();
			} else if (adapter.id === "node") {
				version = await getNodeVersion();
			} else if (adapter.id === "go") {
				version = await getDlvVersion();
			} else if (adapter.id === "rust") {
				version = await getCargoVersion();
			} else if (adapter.id === "java") {
				version = await getJavacVersion();
			} else if (adapter.id === "cpp") {
				version = await getGdbVersion();
			} else if (adapter.id === "ruby") {
				version = await getRdbgVersion();
			} else if (adapter.id === "csharp") {
				version = await getNetcoredbgVersion();
			} else if (adapter.id === "swift") {
				version = await getSwiftcVersion();
			} else if (adapter.id === "kotlin") {
				version = await getKotlincVersion();
			}
			adapterResults.push({
				id: adapter.id,
				displayName: adapter.displayName,
				status: "available",
				version,
			});
		} else {
			adapterResults.push({
				id: adapter.id,
				displayName: adapter.displayName,
				status: "missing",
				installHint: prereq.installHint,
				fixCommand: prereq.fixCommand,
			});
		}
	}

	registerAllDetectors();
	const frameworkResults: DoctorResult["frameworks"] = listDetectors().map((d) => ({
		id: d.id,
		displayName: d.displayName,
		adapterId: d.adapterId,
	}));

	// Check for .vscode/launch.json in cwd
	let launchConfigs: DoctorResult["launchConfigs"];
	const launchJson = await parseLaunchJson(resolvePath(process.cwd(), ".vscode/launch.json"));
	if (launchJson) {
		launchConfigs = listConfigurations(launchJson);
	}

	return { platform, runtime, runtimeVersion, adapters: adapterResults, frameworks: frameworkResults, launchConfigs };
}

async function getPythonDebugpyVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("python3", ["-m", "debugpy", "--version"], { stdio: "pipe" });
			let stdout = "";
			let stderr = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				stdout += chunk.toString();
			});
			proc.stderr.on("data", (chunk: Buffer) => {
				stderr += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) {
					resolve((stdout + stderr).trim());
				} else {
					reject(new Error("Non-zero exit"));
				}
			});
			proc.on("error", reject);
		});
		// debugpy outputs version like "1.8.0"
		return result.trim() || undefined;
	} catch {
		return undefined;
	}
}

async function getNodeVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("node", ["--version"], { stdio: "pipe" });
			let stdout = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				stdout += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(stdout.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Strip leading "v": "v20.11.0" => "20.11.0"
		return result.replace(/^v/, "") || undefined;
	} catch {
		return undefined;
	}
}

async function getDlvVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const { homedir } = await import("node:os");
		const { join } = await import("node:path");
		// Augment PATH with common Go bin locations, same as the Go adapter does
		const goBin = process.env.GOPATH ? join(process.env.GOPATH, "bin") : join(homedir(), "go", "bin");
		const currentPath = process.env.PATH ?? "";
		const augmentedPath = currentPath.includes(goBin) ? currentPath : `${goBin}:${currentPath}`;
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("dlv", ["version"], { stdio: "pipe", env: { ...process.env, PATH: augmentedPath } });
			let stdout = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				stdout += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(stdout.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "Version: 1.23.0" from dlv version output
		const match = result.match(/Version:\s+(\S+)/);
		return match ? match[1] : result.split("\n")[0] || undefined;
	} catch {
		return undefined;
	}
}

async function getCargoVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("cargo", ["--version"], { stdio: "pipe" });
			let stdout = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				stdout += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(stdout.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "cargo 1.75.0 (..." => "1.75.0"
		const match = result.match(/cargo\s+(\S+)/);
		return match ? match[1] : undefined;
	} catch {
		return undefined;
	}
}

async function getJavacVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("javac", ["-version"], { stdio: "pipe" });
			let output = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.stderr.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(output.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "javac 17.0.8" => "17.0.8"
		const match = result.match(/javac\s+(\S+)/);
		return match ? match[1] : undefined;
	} catch {
		return undefined;
	}
}

async function getGdbVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("gdb", ["--version"], { stdio: "pipe" });
			let stdout = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				stdout += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(stdout.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "GNU gdb ... 14.1" => "14.1"
		const match = result.match(/GNU gdb[^\d]*(\d+\.\d+)/);
		return match ? match[1] : result.split("\n")[0] || undefined;
	} catch {
		return undefined;
	}
}

async function getRdbgVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("rdbg", ["--version"], { stdio: "pipe" });
			let output = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.stderr?.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(output.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "rdbg 1.9.0" => "1.9.0"
		const match = result.match(/rdbg\s+(\S+)/i);
		return match ? match[1] : result.split("\n")[0] || undefined;
	} catch {
		return undefined;
	}
}

async function getNetcoredbgVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const { existsSync } = await import("node:fs");
		const { platform } = await import("node:os");
		const { getKrometrailSubdir: subdir } = await import("../../core/paths.js");

		// Find netcoredbg binary: PATH first, then cache
		const ext = platform() === "win32" ? ".exe" : "";
		const cached = `${subdir("adapters", "netcoredbg")}/netcoredbg${ext}`;
		const cmd = existsSync(cached) ? cached : "netcoredbg";

		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn(cmd, ["--version"], { stdio: "pipe" });
			let output = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.stderr?.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(output.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		return result.split("\n")[0]?.trim() || undefined;
	} catch {
		return undefined;
	}
}

async function getSwiftcVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("swiftc", ["--version"], { stdio: "pipe" });
			let output = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.stderr?.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(output.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "Swift version 5.10 ..." => "5.10"
		const match = result.match(/Swift version\s+(\S+)/i);
		return match ? match[1] : result.split("\n")[0] || undefined;
	} catch {
		return undefined;
	}
}

async function getKotlincVersion(): Promise<string | undefined> {
	try {
		const { spawn } = await import("node:child_process");
		const result = await new Promise<string>((resolve, reject) => {
			const proc = spawn("kotlinc", ["-version"], { stdio: "pipe" });
			let output = "";
			proc.stdout.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.stderr.on("data", (chunk: Buffer) => {
				output += chunk.toString();
			});
			proc.on("close", (code) => {
				if (code === 0) resolve(output.trim());
				else reject(new Error("Non-zero exit"));
			});
			proc.on("error", reject);
		});
		// Parse "info: kotlinc-jvm 2.0.0 (JRE ...)" => "2.0.0"
		const match = result.match(/kotlinc-jvm\s+(\S+)/i);
		return match ? match[1] : result.split("\n")[0] || undefined;
	} catch {
		return undefined;
	}
}

/**
 * Format doctor results for the chosen output mode.
 */
export function formatDoctor(result: DoctorResult, mode: OutputMode): string {
	if (mode === "json") {
		return successEnvelope(result);
	}

	const lines: string[] = [`Krometrail v0.1.0`, `Platform: ${result.platform}`, `Runtime: ${result.runtime} ${result.runtimeVersion}`, "", "Adapters:"];

	for (const adapter of result.adapters) {
		if (adapter.status === "available") {
			const version = adapter.version ? `  v${adapter.version}` : "";
			lines.push(`  [OK]  ${adapter.displayName.padEnd(22)}${version}`);
		} else {
			const hint = adapter.installHint ? `  not installed — ${adapter.installHint}` : "  not installed";
			lines.push(`  [--]  ${adapter.displayName.padEnd(22)}${hint}`);
		}
	}

	lines.push("", "Framework Detectors:");
	for (const fw of result.frameworks) {
		lines.push(`  ${fw.displayName.padEnd(24)}(${fw.adapterId})`);
	}

	if (result.launchConfigs !== undefined) {
		lines.push("", "launch.json Configurations:");
		if (result.launchConfigs.length === 0) {
			lines.push("  (none found)");
		} else {
			for (const cfg of result.launchConfigs) {
				lines.push(`  ${cfg.name.padEnd(32)}[${cfg.type}/${cfg.request}]`);
			}
		}
	}

	return lines.join("\n");
}

export const doctorCommand = defineCommand({
	meta: {
		name: "doctor",
		description: "Check installed debuggers and system readiness",
	},
	args: {
		json: {
			type: "boolean",
			description: "Output as JSON",
			default: false,
		},
		quiet: {
			type: "boolean",
			description: "Minimal output",
			default: false,
		},
		fix: {
			type: "boolean",
			description: "Print fix commands for missing adapters",
			default: false,
		},
	},
	async run({ args }) {
		const mode = resolveOutputMode(args);

		// Register adapters and detectors directly (doctor doesn't need the daemon)
		registerAllAdapters();

		const result = await runDoctorChecks();

		if (args.fix) {
			const missing = result.adapters.filter((a) => a.status === "missing" && a.fixCommand);
			if (missing.length === 0) {
				process.stdout.write("All adapters are available — nothing to fix.\n");
				process.exit(0);
			}
			process.stdout.write("# Run these commands to install missing debuggers:\n\n");
			for (const adapter of missing) {
				process.stdout.write(`# ${adapter.displayName}\n${adapter.fixCommand}\n\n`);
			}
			process.exit(0);
		}

		process.stdout.write(`${formatDoctor(result, mode)}\n`);

		// Exit code: 0 if at least one adapter available, 1 if none
		const hasAvailable = result.adapters.some((a) => a.status === "available");
		process.exit(hasAvailable ? 0 : 1);
	},
});
