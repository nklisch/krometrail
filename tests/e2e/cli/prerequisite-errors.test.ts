import { execSync, spawn } from "node:child_process";
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const PROJECT_ROOT = resolve(import.meta.dirname, "../../../");
const IMAGE_TAG = "krometrail-prereq-test";

/** Run a command inside the test container and capture output. */
function dockerRun(args: string, opts?: { timeoutMs?: number }): Promise<{ exitCode: number; stdout: string; stderr: string }> {
	return new Promise((resolve, reject) => {
		const proc = spawn("docker", ["run", "--rm", IMAGE_TAG, "sh", "-c", args], {
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
		const timer = setTimeout(() => {
			proc.kill();
			reject(new Error("docker run timed out"));
		}, opts?.timeoutMs ?? 30_000);
		proc.on("close", (code) => {
			clearTimeout(timer);
			resolve({ exitCode: code ?? 1, stdout, stderr });
		});
		proc.on("error", (err) => {
			clearTimeout(timer);
			reject(err);
		});
	});
}

/** Check if docker is available. */
function isDockerAvailable(): boolean {
	try {
		execSync("docker info", { stdio: "ignore", timeout: 5_000 });
		return true;
	} catch {
		return false;
	}
}

const SKIP_NO_DOCKER = !isDockerAvailable();

describe.skipIf(SKIP_NO_DOCKER)("E2E: prerequisite and adapter errors (containerized)", () => {
	beforeAll(() => {
		// Build a minimal image: bun + krometrail source, NO debuggers installed
		const dockerfilePath = resolve(import.meta.dirname, "Dockerfile.prereq-test");
		execSync(`docker build -t ${IMAGE_TAG} -f ${dockerfilePath} ${PROJECT_ROOT}`, {
			stdio: "inherit",
			timeout: 120_000,
		});
	}, 180_000);

	afterAll(() => {
		try {
			execSync(`docker rmi ${IMAGE_TAG}`, { stdio: "ignore" });
		} catch {
			// ignore cleanup failures
		}
	});

	describe("missing adapter prerequisites", () => {
		it("launch python with no debugpy returns exit code 6", async () => {
			const result = await dockerRun('bun run src/cli/index.ts debug launch "python3 /tmp/test.py" 2>&1; echo "EXIT:$?"');
			const exitMatch = result.stdout.match(/EXIT:(\d+)/);
			const exitCode = exitMatch ? Number(exitMatch[1]) : -1;
			expect(exitCode).toBe(6);
			expect(result.stdout).toContain("prerequisites not met");
		}, 30_000);

		it("launch python --json returns ADAPTER_PREREQUISITES error", async () => {
			const result = await dockerRun('bun run src/cli/index.ts debug launch "python3 /tmp/test.py" --json 2>&1');
			// JSON envelope is on stdout or stderr
			const output = result.stdout + result.stderr;
			const jsonMatch = output.match(/\{[\s\S]*"ok"\s*:\s*false[\s\S]*\}/);
			expect(jsonMatch).toBeTruthy();
			if (jsonMatch) {
				const parsed = JSON.parse(jsonMatch[0]);
				expect(parsed.ok).toBe(false);
				expect(parsed.error.code).toBe("ADAPTER_PREREQUISITES");
				expect(parsed.error.retryable).toBe(false);
			}
		}, 30_000);

		it("prerequisite error in text mode mentions krometrail doctor", async () => {
			const result = await dockerRun('bun run src/cli/index.ts debug launch "python3 /tmp/test.py" 2>&1');
			expect(result.stdout).toContain("krometrail doctor");
		}, 30_000);
	});

	describe("unknown language/extension", () => {
		it("launch with unknown extension returns exit code 3", async () => {
			const result = await dockerRun('bun run src/cli/index.ts debug launch "unknown.xyz" 2>&1; echo "EXIT:$?"');
			const exitMatch = result.stdout.match(/EXIT:(\d+)/);
			const exitCode = exitMatch ? Number(exitMatch[1]) : -1;
			expect(exitCode).toBe(3);
			expect(result.stdout).toContain("krometrail doctor");
			expect(result.stdout).not.toContain("debug_status");
		}, 30_000);
	});

	describe("doctor --fix", () => {
		it("doctor --fix prints fix commands for missing adapters", async () => {
			const result = await dockerRun("bun run src/cli/index.ts doctor --fix");
			expect(result.stdout).toContain("# Run these commands");
		}, 30_000);

		it("doctor --json includes fixCommand in missing adapter entries", async () => {
			const result = await dockerRun("bun run src/cli/index.ts doctor --json 2>&1");
			const output = result.stdout + result.stderr;
			const jsonMatch = output.match(/\{[\s\S]*"ok"\s*:\s*true[\s\S]*\}/);
			expect(jsonMatch).toBeTruthy();
			if (jsonMatch) {
				const parsed = JSON.parse(jsonMatch[0]);
				expect(parsed.ok).toBe(true);
				const missing = parsed.data.adapters.filter((a: { status: string }) => a.status === "missing");
				// In the minimal container, most adapters should be missing
				expect(missing.length).toBeGreaterThan(5);
				for (const adapter of missing) {
					expect(adapter.installHint, `Adapter '${adapter.id}' is missing but has no installHint`).toBeTruthy();
				}
			}
		}, 30_000);
	});
});
