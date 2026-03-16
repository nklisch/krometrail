import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { runCli } from "../../helpers/cli-runner.js";

const CLI_ENTRY = resolve(import.meta.dirname, "../../../src/cli/index.ts");
const PROJECT_ROOT = resolve(import.meta.dirname, "../../../");

/**
 * Spawn the CLI with given args, wait briefly, check it's still alive, then kill it.
 * Returns { alive: boolean, stdout: string, stderr: string }.
 */
function spawnAndCheck(args: string[], waitMs = 500): Promise<{ alive: boolean; stdout: string; stderr: string; exitCode: number | null }> {
	return new Promise((resolve) => {
		const proc = spawn("bun", ["run", CLI_ENTRY, ...args], {
			stdio: ["pipe", "pipe", "pipe"],
			env: { ...process.env },
		});

		let stdout = "";
		let stderr = "";
		let exited = false;
		let exitCode: number | null = null;

		proc.stdout.on("data", (chunk: Buffer) => {
			stdout += chunk.toString();
		});

		proc.stderr.on("data", (chunk: Buffer) => {
			stderr += chunk.toString();
		});

		proc.on("close", (code) => {
			exited = true;
			exitCode = code;
		});

		setTimeout(() => {
			const alive = !exited;
			if (alive) {
				proc.kill("SIGTERM");
			}
			// Give it a moment to clean up
			setTimeout(() => {
				resolve({ alive, stdout, stderr, exitCode });
			}, 100);
		}, waitMs);
	});
}

/**
 * Run an arbitrary command and capture output.
 */
function runShell(cmd: string, args: string[], opts?: { env?: Record<string, string> }): Promise<{ exitCode: number; stdout: string; stderr: string }> {
	return new Promise((resolve) => {
		const proc = spawn(cmd, args, {
			stdio: ["ignore", "pipe", "pipe"],
			env: opts?.env ? { ...process.env, ...opts.env } : undefined,
		});
		let stdout = "";
		let stderr = "";
		proc.stdout.on("data", (chunk: Buffer) => {
			stdout += chunk.toString();
		});
		proc.stderr.on("data", (chunk: Buffer) => {
			stderr += chunk.toString();
		});
		proc.on("close", (code) => resolve({ exitCode: code ?? 1, stdout, stderr }));
	});
}

describe("E2E: installation claims", () => {
	describe("CLI basics", () => {
		it("krometrail --version exits 0 and outputs semver", async () => {
			const result = await runCli(["--version"]);
			expect(result.exitCode).toBe(0);
			const output = (result.stdout + result.stderr).trim();
			expect(output).toMatch(/\d+\.\d+\.\d+/);
		});

		it("krometrail doctor exits 0 with expected sections", async () => {
			const result = await runCli(["doctor"]);
			expect(result.exitCode).toBe(0);
			expect(result.stdout).toContain("Krometrail");
			expect(result.stdout).toContain("Platform:");
			expect(result.stdout).toContain("Adapters:");
		});

		it("krometrail --help mentions debug, browser, and doctor", async () => {
			const result = await runCli(["--help"]);
			const output = (result.stdout + result.stderr).toLowerCase();
			expect(output).toContain("debug");
			expect(output).toContain("browser");
			expect(output).toContain("doctor");
		});

		it("krometrail with no args shows help output", async () => {
			const result = await runCli([]);
			expect(result.exitCode).toBe(0);
			const output = (result.stdout + result.stderr).toLowerCase();
			expect(output).toContain("debug");
			expect(output).toContain("browser");
			expect(output).toContain("doctor");
			expect(output).toContain("usage");
		});
	});

	describe("MCP server startup", () => {
		it("krometrail --mcp starts and stays alive", async () => {
			const result = await spawnAndCheck(["--mcp"]);
			expect(result.alive).toBe(true);
		}, 10_000);

		// Use --tools=X syntax to prevent citty from interpreting the value as a subcommand
		it("krometrail --mcp --tools=debug starts without error", async () => {
			const result = await spawnAndCheck(["--mcp", "--tools=debug"]);
			expect(result.alive).toBe(true);
		}, 10_000);

		it("krometrail --mcp --tools=browser starts without error", async () => {
			const result = await spawnAndCheck(["--mcp", "--tools=browser"]);
			expect(result.alive).toBe(true);
		}, 10_000);
	});

	describe("install script", () => {
		const installScript = resolve(PROJECT_ROOT, "scripts/install.sh");

		it("install script is valid shell syntax", async () => {
			const result = await runShell("sh", ["-n", installScript]);
			expect(result.exitCode).toBe(0);
		});

		it("install script supports --help flag", async () => {
			const result = await runShell("sh", [installScript, "--help"]);
			expect(result.exitCode).toBe(0);
			const output = (result.stdout + result.stderr).toLowerCase();
			expect(output).toMatch(/usage|install/);
		});

		it("install script accepts --version flag without crashing on parse", async () => {
			// --version with a bogus value will try to download and fail,
			// but it should parse the flag correctly (not error on unknown flag)
			const result = await runShell("sh", [installScript, "--version", "v99.99.99"]);
			const stderrLower = result.stderr.toLowerCase();
			expect(stderrLower).not.toContain("unknown flag");
			expect(stderrLower).not.toContain("unknown option");
		});
	});

	describe("documentation config validity", () => {
		const docFiles = ["docs/guide/mcp-configuration.md", "docs/guide/getting-started.md", "docs/guides/claude-code.md", "docs/guides/cursor-windsurf.md", "docs/guides/codex.md", "README.md"].map(
			(f) => resolve(PROJECT_ROOT, f),
		);

		/**
		 * Extract fenced JSON blocks from markdown content.
		 */
		function extractJsonBlocks(content: string): string[] {
			const blocks: string[] = [];
			const regex = /```json\s*\n([\s\S]*?)```/g;
			let match: RegExpExecArray | null;
			while ((match = regex.exec(content)) !== null) {
				blocks.push(match[1].trim());
			}
			return blocks;
		}

		/**
		 * Extract fenced TOML blocks from markdown content.
		 */
		function extractTomlBlocks(content: string): string[] {
			const blocks: string[] = [];
			const regex = /```toml\s*\n([\s\S]*?)```/g;
			let match: RegExpExecArray | null;
			while ((match = regex.exec(content)) !== null) {
				blocks.push(match[1].trim());
			}
			return blocks;
		}

		/**
		 * Try to parse a JSON block. Some doc blocks contain multiple JSON examples
		 * on separate lines (not a single JSON value) — skip those gracefully.
		 */
		function tryParseJson(block: string): { parsed: unknown; valid: boolean } {
			try {
				return { parsed: JSON.parse(block), valid: true };
			} catch {
				return { parsed: undefined, valid: false };
			}
		}

		it("JSON config blocks with mcpServers are valid JSON", () => {
			let configBlocks = 0;
			for (const filePath of docFiles) {
				if (!existsSync(filePath)) continue;
				const content = readFileSync(filePath, "utf-8");
				const blocks = extractJsonBlocks(content);
				for (const block of blocks) {
					const { parsed, valid } = tryParseJson(block);
					if (!valid) {
						// Non-parseable JSON blocks are OK if they're example snippets
						// (e.g., multiple JSON objects on separate lines for illustration).
						// But they must not look like config blocks (containing mcpServers).
						expect(block.includes("mcpServers"), `Unparseable JSON block in ${filePath} appears to be an MCP config but is invalid JSON:\n${block.slice(0, 200)}`).toBe(false);
						continue;
					}
					if (parsed && typeof parsed === "object" && "mcpServers" in (parsed as Record<string, unknown>)) {
						configBlocks++;
					}
				}
			}
			// We expect at least some mcpServers config blocks across all docs
			expect(configBlocks).toBeGreaterThan(0);
		});

		it("JSON mcpServers configs use --mcp in args (not bare mcp)", () => {
			for (const filePath of docFiles) {
				if (!existsSync(filePath)) continue;
				const content = readFileSync(filePath, "utf-8");
				const blocks = extractJsonBlocks(content);
				for (const block of blocks) {
					const { parsed, valid } = tryParseJson(block);
					if (!valid) continue;

					// Check if this is an MCP config with mcpServers
					if (parsed && typeof parsed === "object" && "mcpServers" in (parsed as Record<string, unknown>)) {
						const servers = (parsed as Record<string, unknown>).mcpServers as Record<string, unknown>;
						for (const [serverName, config] of Object.entries(servers)) {
							if (!config || typeof config !== "object") continue;
							const serverConfig = config as Record<string, unknown>;
							if (Array.isArray(serverConfig.args)) {
								const args = serverConfig.args as string[];
								const hasBareMcp = args.some((a) => a === "mcp");
								const hasDashMcp = args.some((a) => a === "--mcp");
								if (serverName.toLowerCase().includes("krometrail") || hasBareMcp || hasDashMcp) {
									expect(hasDashMcp, `Config for "${serverName}" in ${filePath} should use "--mcp" in args, got: ${JSON.stringify(args)}`).toBe(true);
									expect(hasBareMcp, `Config for "${serverName}" in ${filePath} should NOT use bare "mcp" in args, got: ${JSON.stringify(args)}`).toBe(false);
								}
							}
						}
					}
				}
			}
		});

		it("TOML config blocks use --mcp in args", () => {
			let checked = 0;
			for (const filePath of docFiles) {
				if (!existsSync(filePath)) continue;
				const content = readFileSync(filePath, "utf-8");
				const blocks = extractTomlBlocks(content);
				for (const block of blocks) {
					// Look for args lines that reference krometrail (MCP config)
					const argsLines = block.split("\n").filter((line) => /^\s*args\s*=/.test(line));
					for (const line of argsLines) {
						// Only check lines that are krometrail-related
						if (line.includes("krometrail")) {
							checked++;
							// Extract the individual arg strings from the TOML array
							const argStrings = [...line.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
							const hasBareMcp = argStrings.some((a) => a === "mcp");
							const hasDashMcp = argStrings.some((a) => a === "--mcp");
							expect(hasDashMcp, `TOML args in ${filePath} should include "--mcp": ${line.trim()}`).toBe(true);
							expect(hasBareMcp, `TOML args in ${filePath} should NOT include bare "mcp": ${line.trim()}`).toBe(false);
						}
					}
				}
			}
			expect(checked).toBeGreaterThan(0);
		});

		it("claude mcp add command examples use --mcp", () => {
			for (const filePath of docFiles) {
				if (!existsSync(filePath)) continue;
				const content = readFileSync(filePath, "utf-8");
				const lines = content.split("\n");
				for (const line of lines) {
					if (line.includes("claude mcp add") && line.includes("krometrail")) {
						expect(line.includes("--mcp"), `"claude mcp add" command in ${filePath} should use "--mcp": ${line.trim()}`).toBe(true);
						// Ensure the args after "-- " don't have bare "mcp" as a standalone token
						const afterDash = line.split("-- ")[1] || "";
						if (afterDash) {
							const argTokens = afterDash.trim().split(/\s+/);
							const bareMcpArgs = argTokens.filter((t) => t === "mcp");
							expect(bareMcpArgs.length, `"claude mcp add" in ${filePath} should not have bare "mcp" after "--": ${line.trim()}`).toBe(0);
						}
					}
				}
			}
		});

		it("codex mcp add command examples use --mcp", () => {
			for (const filePath of docFiles) {
				if (!existsSync(filePath)) continue;
				const content = readFileSync(filePath, "utf-8");
				const lines = content.split("\n");
				for (const line of lines) {
					if (line.includes("codex mcp add") && line.includes("krometrail")) {
						expect(line.includes("--mcp"), `"codex mcp add" command in ${filePath} should use "--mcp": ${line.trim()}`).toBe(true);
						const afterDash = line.split("-- ")[1] || "";
						if (afterDash) {
							const argTokens = afterDash.trim().split(/\s+/);
							const bareMcpArgs = argTokens.filter((t) => t === "mcp");
							expect(bareMcpArgs.length, `"codex mcp add" in ${filePath} should not have bare "mcp" after "--": ${line.trim()}`).toBe(0);
						}
					}
				}
			}
		});
	});
});
