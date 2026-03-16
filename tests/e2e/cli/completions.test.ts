import { spawn } from "node:child_process";
import { rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { runCli } from "../../helpers/cli-runner.js";

/**
 * Run bash -n on a script string to validate syntax.
 * Writes to a temp file, runs bash -n, then cleans up.
 */
async function validateBashSyntax(script: string): Promise<{ exitCode: number; stderr: string }> {
	const tmpFile = join(tmpdir(), `krometrail-completions-${Date.now()}.bash`);
	writeFileSync(tmpFile, script, "utf-8");
	try {
		return await new Promise((resolve) => {
			const proc = spawn("bash", ["-n", tmpFile], { stdio: ["ignore", "pipe", "pipe"] });
			let stderr = "";
			proc.stderr.on("data", (chunk: Buffer) => {
				stderr += chunk.toString();
			});
			proc.on("close", (code) => resolve({ exitCode: code ?? 1, stderr }));
		});
	} finally {
		try {
			rmSync(tmpFile);
		} catch {
			// ignore cleanup errors
		}
	}
}

describe("E2E CLI: completions", () => {
	it("completions bash outputs valid bash script", async () => {
		const result = await runCli(["completions", "bash"]);
		expect(result.exitCode).toBe(0);
		expect(result.stdout).toContain("complete");
		expect(result.stdout).toContain("_krometrail");

		// Validate syntax with bash -n
		const syntaxCheck = await validateBashSyntax(result.stdout);
		expect(syntaxCheck.exitCode).toBe(0);
	});

	it("completions zsh outputs valid zsh script", async () => {
		const result = await runCli(["completions", "zsh"]);
		expect(result.exitCode).toBe(0);
		expect(result.stdout).toContain("compdef");
		expect(result.stdout).toContain("_krometrail");
	});

	it("completions fish outputs valid fish script", async () => {
		const result = await runCli(["completions", "fish"]);
		expect(result.exitCode).toBe(0);
		expect(result.stdout).toContain("complete -c krometrail");
	});

	it("completions with invalid shell exits 2", async () => {
		const result = await runCli(["completions", "powershell"]);
		expect(result.exitCode).toBe(2);
		expect(result.stderr).toContain("Unknown shell");
	});

	it("completions scripts include all top-level commands", async () => {
		const result = await runCli(["completions", "bash"]);
		for (const cmd of ["debug", "browser", "doctor", "commands", "completions"]) {
			expect(result.stdout).toContain(cmd);
		}
	});

	it("bash completions include __fish_use_subcommand guard absent (sanity check)", async () => {
		const bashResult = await runCli(["completions", "bash"]);
		// bash script should not contain fish-specific syntax
		expect(bashResult.stdout).not.toContain("__fish_use_subcommand");
	});

	it("fish completions include top-level command completions with __fish_use_subcommand", async () => {
		const result = await runCli(["completions", "fish"]);
		expect(result.stdout).toContain("__fish_use_subcommand");
		expect(result.stdout).toContain("__fish_seen_subcommand_from");
	});
});
