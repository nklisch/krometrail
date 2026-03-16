import type { ChildProcess } from "node:child_process";
import { exec, spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, extname, join, resolve as resolvePath } from "node:path";
import { promisify } from "node:util";
import { AdapterInstallError, getErrorMessage, LaunchError } from "../core/errors.js";
import type { AttachConfig, DAPConnection, DebugAdapter, LaunchConfig, PrerequisiteResult } from "./base.js";
import { checkCommandVersioned, downloadError, downloadToFile, getAdapterCacheDir, gracefulDispose } from "./helpers.js";

const execAsync = promisify(exec);

/**
 * kotlin-debug-adapter (fwcd) version pinned here.
 * Communicates via stdin/stdout DAP. Main class: org.javacs.ktda.KDAMainKt
 */
const KDA_VERSION = "0.4.4";
const KDA_URL = `https://github.com/fwcd/kotlin-debug-adapter/releases/download/${KDA_VERSION}/adapter.zip`;
const KDA_MAIN_CLASS = "org.javacs.ktda.KDAMainKt";

function getKdaCacheDir(): string {
	return getAdapterCacheDir("kotlin-debug");
}

function getKdaMarkerJar(): string {
	return join(getKdaCacheDir(), "lib", `adapter-${KDA_VERSION}.jar`);
}

function isKdaCached(): boolean {
	return existsSync(getKdaMarkerJar());
}

/**
 * Build the full classpath for the KDA from all JARs in its lib directory.
 */
function buildKdaClasspath(): string {
	const libDir = join(getKdaCacheDir(), "lib");
	return readdirSync(libDir)
		.filter((f) => f.endsWith(".jar"))
		.map((f) => join(libDir, f))
		.join(":");
}

/**
 * Download and extract the kotlin-debug-adapter zip, caching all lib JARs.
 */
async function downloadAndCacheKda(): Promise<void> {
	const cacheDir = getKdaCacheDir();
	const libDir = join(cacheDir, "lib");
	mkdirSync(libDir, { recursive: true });

	const zipPath = join(tmpdir(), `kotlin-debug-adapter-${KDA_VERSION}.zip`);

	try {
		await downloadToFile(KDA_URL, zipPath, "kotlin-debug-adapter");
	} catch (err) {
		throw downloadError("kotlin-debug-adapter", KDA_VERSION, KDA_URL, libDir, err, `To install manually, download ${KDA_URL} and extract adapter/lib/*.jar to: ${libDir}`);
	}

	// Extract adapter/lib/*.jar into the cache lib directory
	try {
		await execAsync(`unzip -o "${zipPath}" "adapter/lib/*" -d "${cacheDir}/extract"`, { timeout: 30_000 });
		await execAsync(`mv "${cacheDir}/extract/adapter/lib/"*.jar "${libDir}/"`, { timeout: 10_000 });
		await execAsync(`rm -rf "${cacheDir}/extract"`, { timeout: 5_000 });
	} catch (err) {
		throw new AdapterInstallError("kotlin", `Failed to extract archive: ${getErrorMessage(err)}`);
	}
}

export class KotlinAdapter implements DebugAdapter {
	id = "kotlin";
	fileExtensions = [".kt"];
	displayName = "Kotlin (kotlin-debug-adapter)";

	private adapterProcess: ChildProcess | null = null;
	private projectDir: string | null = null;

	/**
	 * Check for kotlinc and JDK 17+ availability, and the cached KDA.
	 */
	async checkPrerequisites(): Promise<PrerequisiteResult> {
		// Check kotlinc (outputs version to stderr)
		const kotlinResult = await checkCommandVersioned({
			cmd: "kotlinc",
			args: ["-version"],
			versionRegex: /kotlinc-jvm\s+(\d+)/,
			missing: ["kotlinc"],
			installHint: "Install Kotlin from https://kotlinlang.org/docs/command-line.html or via SDKMAN: sdk install kotlin",
		});
		if (!kotlinResult.satisfied) return kotlinResult;

		// Check JDK 17+
		const javacResult = await checkCommandVersioned({
			cmd: "javac",
			args: ["-version"],
			versionRegex: /javac\s+(\d+)/,
			minVersion: 17,
			missing: ["javac (17+)"],
			installHint: (v) => (v === 0 ? "Install JDK 17+ from https://adoptium.net" : `JDK ${v} is too old. Install JDK 17+ from https://adoptium.net`),
		});
		if (!javacResult.satisfied) return javacResult;

		// Check kotlin-debug-adapter (downloaded automatically on first use)
		if (!isKdaCached()) {
			return {
				satisfied: false,
				missing: ["kotlin-debug-adapter"],
				installHint: "The kotlin-debug-adapter will be downloaded automatically on first use.",
			};
		}

		return { satisfied: true };
	}

	/**
	 * Launch a Kotlin program via the kotlin-debug-adapter (stdio DAP transport).
	 * Compiles .kt source files with kotlinc -include-runtime, then launches KDA.
	 */
	async launch(config: LaunchConfig): Promise<DAPConnection> {
		const cwd = config.cwd ?? process.cwd();
		const parsed = parseKotlinCommand(config.command);

		// KDA uses projectRoot to discover compiled classes via a Gradle-like structure:
		// projectRoot/build/classes/kotlin/main/<classfiles>
		// It ignores `classPaths` from the launch request entirely.
		const projectDir = join(tmpdir(), `krometrail-kda-${Date.now()}`);
		const classOutputDir = join(projectDir, "build", "classes", "kotlin", "main");
		mkdirSync(classOutputDir, { recursive: true });
		this.projectDir = projectDir;

		let mainClass: string;

		if (parsed.type === "source") {
			const src = resolvePath(cwd, parsed.path);
			mainClass = deriveMainClass(basename(parsed.path));

			// Copy source into the Gradle src/main/kotlin layout so KDA's project-root
			// source scanner can resolve source references in stack frames.
			const srcDir = join(projectDir, "src", "main", "kotlin");
			mkdirSync(srcDir, { recursive: true });
			copyFileSync(src, join(srcDir, basename(src)));

			try {
				// Compile to class directory (no -include-runtime) — KDA resolves kotlin-stdlib itself.
				await execAsync(`kotlinc "${src}" -d "${classOutputDir}"`, {
					cwd,
					env: { ...process.env, ...config.env },
					timeout: 60_000,
				});
			} catch (err) {
				throw new LaunchError(`kotlinc compilation failed: ${getErrorMessage(err)}`);
			}
		} else if (parsed.type === "jar") {
			// For pre-compiled JARs, put them where ProjectClassesResolver looks.
			// Copy class files from JAR into the expected directory structure.
			const jarPath = resolvePath(cwd, parsed.path);
			try {
				await execAsync(`jar xf "${jarPath}"`, { cwd: classOutputDir, timeout: 10_000 });
			} catch (err) {
				throw new LaunchError(`Failed to extract JAR: ${getErrorMessage(err)}`);
			}
			mainClass = basename(parsed.path, ".jar");
		} else {
			mainClass = parsed.path;
		}

		// Ensure kotlin-debug-adapter is cached
		if (!isKdaCached()) {
			await downloadAndCacheKda();
		}

		const classpath = buildKdaClasspath();

		// Spawn KDA — communicates via stdin/stdout DAP
		const child = spawn("java", ["-classpath", classpath, KDA_MAIN_CLASS], {
			cwd,
			env: { ...process.env, ...config.env },
			stdio: ["pipe", "pipe", "pipe"],
		});

		this.adapterProcess = child;

		// Check for early spawn failure
		const earlyError = await new Promise<Error | null>((resolve) => {
			const stderrChunks: string[] = [];
			child.stderr?.on("data", (d: Buffer) => stderrChunks.push(d.toString()));
			child.on("error", (err) => resolve(new LaunchError(`Failed to spawn kotlin-debug-adapter: ${err.message}`)));
			child.on("close", (code) => {
				if (code !== null && code !== 0) {
					resolve(new LaunchError(`kotlin-debug-adapter exited with code ${code}. output: ${stderrChunks.join("")}`));
				} else {
					resolve(null);
				}
			});
			setTimeout(() => resolve(null), 1_000);
		});

		if (earlyError) throw earlyError;
		if (!child.stdout || !child.stdin) throw new LaunchError("kotlin-debug-adapter stdio not available");

		return {
			reader: child.stdout,
			writer: child.stdin,
			process: child,
			launchArgs: {
				// KDA needs launch before setBreakpoints to trigger JVM resolution.
				// KDA never responds to configurationDone, so fire it without awaiting.
				_dapFlow: "launch-first",
				_fireConfigDone: true,
				mainClass,
				projectRoot: projectDir,
				enableJsonLogging: false,
			},
		};
	}

	/**
	 * Attach to a JVM process with JDWP agent enabled.
	 */
	async attach(config: AttachConfig): Promise<DAPConnection> {
		const host = config.host ?? "127.0.0.1";
		const jdwpPort = config.port ?? 5005;

		if (!isKdaCached()) {
			await downloadAndCacheKda();
		}

		const classpath = buildKdaClasspath();

		const child = spawn("java", ["-classpath", classpath, KDA_MAIN_CLASS], {
			stdio: ["pipe", "pipe", "pipe"],
			env: { ...process.env },
		});

		this.adapterProcess = child;
		if (!child.stdout || !child.stdin) throw new LaunchError("kotlin-debug-adapter stdio not available");

		return {
			reader: child.stdout,
			writer: child.stdin,
			process: child,
			launchArgs: {
				request: "attach",
				hostName: host,
				port: jdwpPort,
			},
		};
	}

	async dispose(): Promise<void> {
		await gracefulDispose(null, this.adapterProcess);
		this.adapterProcess = null;
		if (this.projectDir) {
			try {
				rmSync(this.projectDir, { recursive: true, force: true });
			} catch {}
			this.projectDir = null;
		}
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
