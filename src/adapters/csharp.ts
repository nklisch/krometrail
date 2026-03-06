import type { ChildProcess } from "node:child_process";
import { exec, spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import type { Socket } from "node:net";
import { tmpdir } from "node:os";
import { basename, extname, join, resolve as resolvePath } from "node:path";
import { promisify } from "node:util";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, gracefulDispose, spawnAndWait } from "./helpers.js";
import { downloadAndCacheNetcoredbg, getNetcoredbgBinaryPath, isNetcoredbgCached } from "./netcoredbg.js";

const execAsync = promisify(exec);

export class CSharpAdapter implements DebugAdapter {
	id = "csharp";
	fileExtensions = [".cs"];
	displayName = "C# (netcoredbg)";

	private adapterProcess: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for dotnet CLI and netcoredbg availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		const dotnetOk = await new Promise<boolean>((resolve) => {
			const proc = spawn("dotnet", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});

		if (!dotnetOk) {
			return {
				satisfied: false,
				missing: ["dotnet"],
				installHint: "Install .NET SDK from https://dotnet.microsoft.com/download",
			};
		}

		// Check netcoredbg: PATH first, then cache
		const onPath = await new Promise<boolean>((resolve) => {
			const proc = spawn("netcoredbg", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});

		if (!onPath && !isNetcoredbgCached()) {
			return {
				satisfied: false,
				missing: ["netcoredbg"],
				installHint: "Will be downloaded automatically on first use, or install from https://github.com/Samsung/netcoredbg/releases",
			};
		}

		return { satisfied: true };
	}

	/**
	 * Launch a C# program via netcoredbg DAP TCP server.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const cwd = config.cwd ?? process.cwd();
		const parsed = parseCSharpCommand(config.command);
		let dllPath: string;

		if (parsed.type === "source") {
			// Single .cs file: scaffold a temp project and build it
			dllPath = await compileSingleCsFile(resolvePath(cwd, parsed.path), config.env);
		} else if (parsed.type === "project") {
			// dotnet run / project directory: build and find the DLL
			const projectPath = resolvePath(cwd, parsed.path);
			const outDir = join(tmpdir(), `agent-lens-cs-${Date.now()}`);
			mkdirSync(outDir, { recursive: true });
			try {
				await execAsync(`dotnet build "${projectPath}" -o "${outDir}" --nologo -v quiet`, {
					cwd,
					env: { ...process.env, ...config.env },
				});
			} catch (err) {
				throw new LaunchError(`dotnet build failed: ${err instanceof Error ? err.message : String(err)}`);
			}
			// Find the main DLL (excludes *.deps.json, *.runtimeconfig.json etc.)
			const { stdout } = await execAsync(`ls "${outDir}"/*.dll | grep -v 'deps\\.json\\|runtimeconfig' | head -1`).catch(() => ({ stdout: "" }));
			const found = stdout.trim();
			if (!found) throw new LaunchError(`No DLL found in build output directory: ${outDir}`);
			dllPath = found;
		} else if (parsed.type === "dll") {
			dllPath = resolvePath(cwd, parsed.path);
		} else {
			// binary
			dllPath = resolvePath(cwd, parsed.path);
		}

		// Ensure netcoredbg is available
		let netcoredbg = "netcoredbg";
		const onPath = await new Promise<boolean>((resolve) => {
			const proc = spawn("netcoredbg", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});
		if (!onPath) {
			if (!isNetcoredbgCached()) {
				await downloadAndCacheNetcoredbg();
			}
			netcoredbg = getNetcoredbgBinaryPath();
		}

		const port = config.port ?? (await allocatePort());

		const { process: adapterProc } = await spawnAndWait({
			cmd: netcoredbg,
			args: ["--interpreter=vscode", "--server", `--server-port=${port}`],
			cwd,
			env: { ...process.env, ...config.env },
			readyPattern: /waiting for connection|started|listening/i,
			timeoutMs: 15_000,
			label: "netcoredbg",
		});

		this.adapterProcess = adapterProc;

		const socket = await connectTCP("127.0.0.1", port, 25, 200).catch((err) => {
			adapterProc.kill();
			throw new LaunchError(`Could not connect to netcoredbg on port ${port}: ${err.message}`);
		});

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs: {
				type: "coreclr",
				program: dllPath,
				args: parsed.args,
				cwd,
				env: config.env ?? {},
				stopAtEntry: false,
				console: "internalConsole",
			},
		};
	}

	/**
	 * Attach to a running .NET process via netcoredbg.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		let netcoredbg = "netcoredbg";
		const onPath = await new Promise<boolean>((resolve) => {
			const proc = spawn("netcoredbg", ["--version"], { stdio: "pipe" });
			proc.on("close", (code) => resolve(code === 0));
			proc.on("error", () => resolve(false));
		});
		if (!onPath) {
			if (!isNetcoredbgCached()) {
				await downloadAndCacheNetcoredbg();
			}
			netcoredbg = getNetcoredbgBinaryPath();
		}

		const port = config.port ?? (await allocatePort());

		const { process: adapterProc } = await spawnAndWait({
			cmd: netcoredbg,
			args: ["--interpreter=vscode", "--server", `--server-port=${port}`],
			readyPattern: /waiting for connection|started|listening/i,
			timeoutMs: 15_000,
			label: "netcoredbg",
		});

		this.adapterProcess = adapterProc;

		const socket = await connectTCP("127.0.0.1", port, 25, 200).catch((err) => {
			adapterProc.kill();
			throw new LaunchError(`Could not connect to netcoredbg on port ${port}: ${err.message}`);
		});

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs: {
				type: "coreclr",
				request: "attach",
				processId: config.pid,
			},
		};
	}

	async dispose(): Promise<void> {
		await gracefulDispose(this.socket, this.adapterProcess);
		this.socket = null;
		this.adapterProcess = null;
	}
}

/**
 * Compile a single .cs file by scaffolding a minimal temporary project.
 * Returns the path to the built DLL.
 */
async function compileSingleCsFile(srcPath: string, env?: Record<string, string>): Promise<string> {
	const projectDir = join(tmpdir(), `agent-lens-cs-${Date.now()}`);
	mkdirSync(projectDir, { recursive: true });

	const projectName = basename(srcPath, ".cs");

	// Scaffold minimal .csproj
	const csproj = `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>`;
	writeFileSync(join(projectDir, `${projectName}.csproj`), csproj);

	// Copy source file into the project
	const { copyFileSync } = await import("node:fs");
	copyFileSync(srcPath, join(projectDir, basename(srcPath)));

	const outDir = join(projectDir, "out");
	mkdirSync(outDir, { recursive: true });

	try {
		await execAsync(`dotnet build "${projectDir}" -o "${outDir}" --nologo -v quiet`, {
			env: { ...process.env, ...env },
		});
	} catch (err) {
		throw new LaunchError(`dotnet build failed for ${srcPath}: ${err instanceof Error ? err.message : String(err)}`);
	}

	const dllPath = join(outDir, `${projectName}.dll`);
	const { existsSync } = await import("node:fs");
	if (!existsSync(dllPath)) {
		throw new LaunchError(`Build succeeded but DLL not found at: ${dllPath}`);
	}

	return dllPath;
}

/**
 * Parse a C# command string.
 * Handles: "dotnet run", "dotnet MyApp.dll", "./MyApp", "MyApp.cs"
 */
export function parseCSharpCommand(command: string): {
	type: "source" | "project" | "dll" | "binary";
	path: string;
	args: string[];
} {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	const first = parts[i] ?? "";

	if (first === "dotnet") {
		i++;
		const sub = parts[i] ?? "";

		if (sub === "run") {
			i++;
			// Check for --project flag
			if (parts[i] === "--project" && parts[i + 1]) {
				return { type: "project", path: parts[i + 1]!, args: parts.slice(i + 2) };
			}
			return { type: "project", path: ".", args: parts.slice(i) };
		}

		// dotnet MyApp.dll
		const path = parts[i] ?? "";
		const ext = extname(path).toLowerCase();
		if (ext === ".dll") {
			return { type: "dll", path, args: parts.slice(i + 1) };
		}
	}

	const path = parts[i] ?? "";
	const ext = extname(path).toLowerCase();

	if (ext === ".cs") {
		return { type: "source", path, args: parts.slice(i + 1) };
	}

	if (ext === ".dll") {
		return { type: "dll", path, args: parts.slice(i + 1) };
	}

	return { type: "binary", path, args: parts.slice(i + 1) };
}
