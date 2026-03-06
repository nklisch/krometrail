import type { ChildProcess } from "node:child_process";
import { exec, spawn } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import type { Socket } from "node:net";
import { tmpdir } from "node:os";
import { basename, extname, resolve as resolvePath } from "node:path";
import { promisify } from "node:util";
import { LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { allocatePort, connectTCP, gracefulDispose, spawnAndWait } from "./helpers.js";
import { downloadAndCacheJavaDebugAdapter, getJavaDebugAdapterCachePath } from "./java.js";

const execAsync = promisify(exec);

function isJavaDebugAdapterCached(): boolean {
	return existsSync(getJavaDebugAdapterCachePath());
}

/**
 * Parse the kotlinc -version output (goes to stderr).
 * Format: "info: kotlinc-jvm 2.0.0 (JRE 21.0.1+12)" — extract major version.
 */
function parseKotlincVersion(output: string): number {
	const match = output.match(/kotlinc-jvm\s+(\d+)/);
	return match ? parseInt(match[1], 10) : 0;
}

/**
 * Parse the javac -version output to extract the major version number.
 */
function parseJavacVersion(output: string): number {
	const match = output.match(/javac\s+(\d+)/);
	return match ? parseInt(match[1], 10) : 0;
}

export class KotlinAdapter implements DebugAdapter {
	id = "kotlin";
	fileExtensions = [".kt"];
	displayName = "Kotlin (java-debug-adapter)";

	private adapterProcess: ChildProcess | null = null;
	private socket: Socket | null = null;

	/**
	 * Check for kotlinc and JDK 17+ availability.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		// Check kotlinc (outputs to stderr)
		const kotlinResult = await new Promise<{ ok: boolean; version: number }>((resolve) => {
			const proc = spawn("kotlinc", ["-version"], { stdio: "pipe" });
			let output = "";
			proc.stdout?.on("data", (d: Buffer) => {
				output += d.toString();
			});
			proc.stderr?.on("data", (d: Buffer) => {
				output += d.toString();
			});
			proc.on("close", (code) => {
				if (code !== 0) {
					resolve({ ok: false, version: 0 });
					return;
				}
				const version = parseKotlincVersion(output);
				resolve({ ok: true, version });
			});
			proc.on("error", () => resolve({ ok: false, version: 0 }));
		});

		if (!kotlinResult.ok) {
			return {
				satisfied: false,
				missing: ["kotlinc"],
				installHint: "Install Kotlin from https://kotlinlang.org/docs/command-line.html or via SDKMAN: sdk install kotlin",
			};
		}

		// Check JDK 17+
		const javacResult = await new Promise<{ ok: boolean; version: number }>((resolve) => {
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
					resolve({ ok: false, version: 0 });
					return;
				}
				const version = parseJavacVersion(output);
				resolve({ ok: true, version });
			});
			proc.on("error", () => resolve({ ok: false, version: 0 }));
		});

		if (!javacResult.ok) {
			return {
				satisfied: false,
				missing: ["javac"],
				installHint: "Install JDK 17+ from https://adoptium.net",
			};
		}

		if (javacResult.version < 17) {
			return {
				satisfied: false,
				missing: ["javac (17+)"],
				installHint: `JDK ${javacResult.version} is too old. Install JDK 17+ from https://adoptium.net`,
			};
		}

		// Check java-debug-adapter JAR
		if (!isJavaDebugAdapterCached()) {
			return {
				satisfied: false,
				missing: ["java-debug-adapter"],
				installHint: "The java-debug-adapter JAR will be downloaded automatically on first use.",
			};
		}

		return { satisfied: true };
	}

	/**
	 * Launch a Kotlin program via the java-debug-adapter.
	 * Compiles .kt source files with kotlinc -include-runtime.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const cwd = config.cwd ?? process.cwd();
		const parsed = parseKotlinCommand(config.command);
		let compiledJarPath: string;

		if (parsed.type === "source") {
			// Compile .kt file to a self-contained JAR
			const src = resolvePath(cwd, parsed.path);
			const outDir = tmpdir();
			const jarName = `agent-lens-kotlin-${Date.now()}.jar`;
			compiledJarPath = `${outDir}/${jarName}`;

			try {
				await execAsync(`kotlinc "${src}" -include-runtime -d "${compiledJarPath}"`, {
					cwd,
					env: { ...process.env, ...config.env },
					timeout: 60_000, // Kotlin compiler can be slow
				});
			} catch (err) {
				throw new LaunchError(`kotlinc compilation failed: ${err instanceof Error ? err.message : String(err)}`);
			}
		} else if (parsed.type === "jar") {
			compiledJarPath = resolvePath(cwd, parsed.path);
		} else {
			// class mode — run main class directly
			compiledJarPath = resolvePath(cwd, parsed.path);
		}

		// Ensure java-debug-adapter JAR is cached
		let jarPath = getJavaDebugAdapterCachePath();
		if (!isJavaDebugAdapterCached()) {
			jarPath = await downloadAndCacheJavaDebugAdapter();
		}

		const port = config.port ?? (await allocatePort());

		const { process: adapterProc } = await spawnAndWait({
			cmd: "java",
			args: ["-jar", jarPath, "--port", String(port)],
			cwd,
			env: { ...process.env, ...config.env },
			readyPattern: /listening|started|ready/i,
			timeoutMs: 20_000,
			label: "java-debug-adapter",
		});

		this.adapterProcess = adapterProc;

		const socket = await connectTCP("127.0.0.1", port, 30, 300).catch((err) => {
			adapterProc.kill();
			throw new LaunchError(`Could not connect to java-debug-adapter on port ${port}: ${err.message}`);
		});

		this.socket = socket;

		const launchArgs: Record<string, unknown> = {
			mainClass: "",
			classPaths: [compiledJarPath],
			jarPath: compiledJarPath,
			cwd,
			env: config.env ?? {},
		};

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs,
		};
	}

	/**
	 * Attach to a JVM process with JDWP agent enabled.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const host = config.host ?? "127.0.0.1";
		const jdwpPort = config.port ?? 5005;

		let jarPath = getJavaDebugAdapterCachePath();
		if (!isJavaDebugAdapterCached()) {
			jarPath = await downloadAndCacheJavaDebugAdapter();
		}

		const dapPort = await allocatePort();

		const { process: adapterProc } = await spawnAndWait({
			cmd: "java",
			args: ["-jar", jarPath, "--port", String(dapPort)],
			readyPattern: /listening|started|ready/i,
			timeoutMs: 20_000,
			label: "java-debug-adapter",
		});

		this.adapterProcess = adapterProc;

		const socket = await connectTCP("127.0.0.1", dapPort, 30, 300).catch((err) => {
			adapterProc.kill();
			throw new LaunchError(`Could not connect to java-debug-adapter on port ${dapPort}: ${err.message}`);
		});

		this.socket = socket;

		return {
			reader: socket,
			writer: socket,
			process: adapterProc,
			launchArgs: {
				request: "attach",
				hostName: host,
				port: jdwpPort,
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
 * Parse a Kotlin command string.
 * Handles: "Main.kt", "kotlinc Main.kt", "java -jar app.jar", "MainKt"
 */
export function parseKotlinCommand(command: string): {
	type: "source" | "jar" | "class";
	path: string;
	args: string[];
} {
	const parts = command.trim().split(/\s+/);
	let i = 0;

	const first = parts[i] ?? "";

	// Strip kotlinc/kotlin/java prefix
	if (first === "kotlinc" || first === "kotlin" || first === "java") {
		i++;
		// Handle "java -jar ..." or "kotlin -jar ..."
		if (parts[i] === "-jar") {
			i++;
			const path = parts[i] ?? "";
			return { type: "jar", path, args: parts.slice(i + 1) };
		}
		// Handle "java -cp ..." — class mode
		if (parts[i] === "-cp" || parts[i] === "-classpath") {
			i += 2; // skip flag and value
		}
	}

	const path = parts[i] ?? "";
	const ext = extname(path).toLowerCase();

	if (ext === ".kt") {
		return { type: "source", path, args: parts.slice(i + 1) };
	}

	if (ext === ".jar") {
		return { type: "jar", path, args: parts.slice(i + 1) };
	}

	// Bare class name
	return { type: "class", path, args: parts.slice(i + 1) };
}

/**
 * Derive the JVM main class name from a .kt filename.
 * "Main.kt" => "MainKt"
 * "hello-world.kt" => "Hello_worldKt"
 */
export function deriveMainClass(filename: string): string {
	const name = basename(filename, ".kt");
	// Replace hyphens with underscores, capitalize first letter
	const sanitized = name.replace(/-/g, "_");
	const capitalized = sanitized.charAt(0).toUpperCase() + sanitized.slice(1);
	return `${capitalized}Kt`;
}
