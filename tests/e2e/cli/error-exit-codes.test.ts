import { execSync, spawn } from "node:child_process";
import { resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { runCli } from "../../helpers/cli-runner.js";

const PROJECT_ROOT = resolve(import.meta.dirname, "../../../");
const IMAGE_TAG = "krometrail-exitcode-test";

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

function isDockerAvailable(): boolean {
	try {
		execSync("docker info", { stdio: "ignore", timeout: 5_000 });
		return true;
	} catch {
		return false;
	}
}

const SKIP_NO_DOCKER = !isDockerAvailable();

describe("E2E: error exit codes", () => {
	it("unknown extension → exit 3 (NOT_FOUND)", async () => {
		const result = await runCli(["debug", "launch", "app.xyz"]);
		expect(result.exitCode).toBe(3);
	});

	it("no active sessions → exit 1 (ERROR)", async () => {
		const result = await runCli(["debug", "continue"]);
		expect(result.exitCode).toBe(1);
	});

	describe.skipIf(SKIP_NO_DOCKER)("containerized exit codes", () => {
		beforeAll(() => {
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
				// ignore
			}
		});

		it("missing prerequisites → exit 6 (PREREQUISITES)", async () => {
			const result = await dockerRun('bun run src/cli/index.ts debug launch "python3 test.py" 2>&1; echo "EXIT:$?"');
			const exitMatch = result.stdout.match(/EXIT:(\d+)/);
			const exitCode = exitMatch ? Number(exitMatch[1]) : -1;
			expect(exitCode).toBe(6);
		}, 30_000);
	});
});
