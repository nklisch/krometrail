import { spawn } from "node:child_process";
import { resolve } from "node:path";

const CLI_ENTRY = resolve(import.meta.dirname, "../../src/cli/index.ts");

export interface CliResult {
	stdout: string;
	stderr: string;
	exitCode: number;
}

/**
 * Run a krometrail CLI command via a child process and capture stdout, stderr, exit code.
 * Does NOT throw on non-zero exit — returns the exit code for assertion.
 *
 * @param args - CLI arguments, e.g. ["debug", "launch", "python app.py", "--json"]
 * @param opts - Optional cwd, env overrides, and timeout
 */
export async function runCli(args: string[], opts?: { cwd?: string; env?: Record<string, string>; timeoutMs?: number }): Promise<CliResult> {
	const timeoutMs = opts?.timeoutMs ?? 30_000;

	return new Promise<CliResult>((resolve, reject) => {
		const proc = spawn("bun", ["run", CLI_ENTRY, ...args], {
			cwd: opts?.cwd,
			env: { ...process.env, ...opts?.env },
			stdio: ["ignore", "pipe", "pipe"],
		});

		let stdout = "";
		let stderr = "";

		proc.stdout.on("data", (chunk: Buffer) => {
			stdout += chunk.toString();
		});

		proc.stderr.on("data", (chunk: Buffer) => {
			stderr += chunk.toString();
		});

		let timedOut = false;
		const timer = setTimeout(() => {
			timedOut = true;
			proc.kill();
			reject(new Error(`CLI command timed out after ${timeoutMs}ms: ${args.join(" ")}`));
		}, timeoutMs);

		proc.on("close", (code) => {
			clearTimeout(timer);
			if (timedOut) return; // already rejected
			resolve({ stdout, stderr, exitCode: code ?? 1 });
		});

		proc.on("error", (err) => {
			clearTimeout(timer);
			if (timedOut) return;
			reject(err);
		});
	});
}

/**
 * Run a CLI command and parse the JSON envelope from stdout (or stderr on error).
 * Success envelopes go to stdout; error envelopes go to stderr.
 * Returns the parsed envelope object.
 */
export async function runCliJson<T = unknown>(
	args: string[],
	opts?: { cwd?: string; env?: Record<string, string>; timeoutMs?: number },
): Promise<{ ok: true; data: T } | { ok: false; error: { code: string; message: string; retryable: boolean } }> {
	const result = await runCli(args, opts);

	// Try stdout first (success envelopes)
	const stdoutTrimmed = result.stdout.trim();
	if (stdoutTrimmed) {
		try {
			const parsed = JSON.parse(stdoutTrimmed);
			if (typeof parsed.ok === "boolean") {
				return parsed as { ok: true; data: T } | { ok: false; error: { code: string; message: string; retryable: boolean } };
			}
		} catch {
			// Not valid JSON from stdout — fall through to stderr
		}
	}

	// Try stderr (error envelopes)
	const stderrTrimmed = result.stderr.trim();
	if (stderrTrimmed) {
		try {
			const parsed = JSON.parse(stderrTrimmed);
			if (typeof parsed.ok === "boolean") {
				return parsed as { ok: true; data: T } | { ok: false; error: { code: string; message: string; retryable: boolean } };
			}
		} catch {
			// Not valid JSON from stderr either
		}
	}

	throw new Error(`CLI command did not return a valid JSON envelope.\nstdout: ${result.stdout.slice(0, 500)}\nstderr: ${result.stderr.slice(0, 500)}\nexitCode: ${result.exitCode}`);
}

/**
 * Helper to extract session ID from text-mode CLI output.
 * Matches "Session started: <id>" or "Session: <id>" with 8 hex chars.
 */
export function extractCliSessionId(text: string): string {
	const match = text.match(/Session (?:started: )?([a-f0-9]{8})/);
	if (!match) throw new Error(`Could not extract CLI session ID from:\n${text.slice(0, 300)}`);
	return match[1];
}
