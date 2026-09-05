import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

interface Step {
	uses?: string;
	run?: string;
	[key: string]: unknown;
}
interface Workflow {
	env?: unknown;
	jobs: Record<string, {
		env?: Record<string, string>;
		steps: Step[];
		[key: string]: unknown;
	}>;
}
interface Manifest {
	package: { "rust-version": string };
	workspace: { package: { "rust-version": string } };
}

const workflowText = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
const manifestText = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
const parseWorkflow = (): Workflow => Bun.YAML.parse(workflowText) as Workflow;
const parseManifest = (): Manifest => Bun.TOML.parse(manifestText) as Manifest;

// This is deliberately a contract for one small job, not a shell interpreter.
// Exact executable steps reject extra/unqualified gates, selection overrides,
// and commented-out commands without pretending to understand arbitrary shell.
function validate(workflow: Workflow, manifest: Manifest): void {
	const minimum = manifest.package["rust-version"];
	expect(minimum).toMatch(/^\d+\.\d+$/);
	expect(manifest.workspace.package["rust-version"]).toBe(minimum);
	const job = workflow.jobs["rust-msrv"];
	expect(workflow.env).toBeUndefined();
	expect(job.env).toEqual({ MSRV_TOOLCHAIN: `${minimum}.0` });
	expect(job["runs-on"]).toBe("ubuntu-latest");
	expect(Object.keys(job).sort()).toEqual(["env", "name", "runs-on", "steps"]);
	const commands = [
		undefined,
		'rustup toolchain install "$MSRV_TOOLCHAIN" --profile minimal',
		'rustup run "$MSRV_TOOLCHAIN" rustc --version\nrustup run "$MSRV_TOOLCHAIN" cargo --version',
		'rustup run "$MSRV_TOOLCHAIN" cargo check --workspace --all-targets --locked',
		'rustup run "$MSRV_TOOLCHAIN" cargo test --workspace --all-targets --locked',
	];
	expect(job.steps.map((step) => step.run?.trim())).toEqual(commands);
	for (const [index, step] of job.steps.entries()) {
		expect(Object.keys(step).sort()).toEqual(index === 0 ? ["name", "uses"] : ["name", "run"]);
	}
	expect(job.steps[0].uses).toBe("actions/checkout@v4");
	const stable = workflow.jobs.rust;
	expect(stable.steps.some((step) => step.uses === "dtolnay/rust-toolchain@stable")).toBe(true);
	for (const command of ["cargo fmt --all --check", "cargo clippy --workspace --all-targets --locked -- -D warnings"]) {
		expect(stable.steps.some((step) => step.run === command)).toBe(true);
	}
}

test("minimum compiler metadata and executable CI steps agree", () => {
	validate(parseWorkflow(), parseManifest());
});

const mutations: [string, (workflow: Workflow, manifest: Manifest) => void][] = [
	["wrong compiler", (w) => { w.jobs["rust-msrv"].env!.MSRV_TOOLCHAIN = "stable"; }],
	["wrong installer compiler", (w) => { w.jobs["rust-msrv"].steps[1].run = "rustup toolchain install stable --profile minimal"; }],
	["workspace metadata drift", (_, m) => { m.workspace.package["rust-version"] = "1.85"; }],
	...[3, 4].map((index): [string, (w: Workflow) => void] => [
		`unqualified ${index === 3 ? "check" : "test"}`,
		(w) => { w.jobs["rust-msrv"].steps[index].run = w.jobs["rust-msrv"].steps[index].run!.replace('rustup run "$MSRV_TOOLCHAIN" ', ""); },
	]),
	...["rustc", "cargo"].map((tool): [string, (w: Workflow) => void] => [
		`missing ${tool} identity`,
		(w) => { w.jobs["rust-msrv"].steps[2].run = w.jobs["rust-msrv"].steps[2].run!.split("\n").filter((line) => !line.includes(`${tool} --version`)).join("\n"); },
	]),
	["step compiler override", (w) => { w.jobs["rust-msrv"].steps[3].env = { RUSTC: "/usr/bin/rustc" }; }],
	["ignored test failure", (w) => { w.jobs["rust-msrv"].steps[4]["continue-on-error"] = true; }],
];
for (const [name, mutate] of mutations) {
	test(`rejects ${name}`, () => {
		const workflow = parseWorkflow();
		const manifest = parseManifest();
		mutate(workflow, manifest);
		expect(() => validate(workflow, manifest)).toThrow();
	});
}
