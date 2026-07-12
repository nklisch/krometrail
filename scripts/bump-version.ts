#!/usr/bin/env bun
// Usage: bun scripts/bump-version.ts [patch|minor|major|x.y.z] [--prepare|--dry-run]

const args = [...process.argv.slice(2)];
const prepare = args.includes("--prepare");
const dryRun = args.includes("--dry-run");
const mode = prepare ? "prepare" : dryRun ? "dry-run" : "release";
const bump = args.find((arg) => !arg.startsWith("--"));

if (!bump || (prepare && dryRun) || args.some((arg) => arg.startsWith("--") && arg !== "--prepare" && arg !== "--dry-run")) {
	console.error("Usage: bun scripts/bump-version.ts [patch|minor|major|x.y.z] [--prepare|--dry-run]");
	process.exit(1);
}

const semverRe = /^(\d+)\.(\d+)\.(\d+)$/;
const cargoPath = "Cargo.toml";
const cargoFile = Bun.file(cargoPath);
const originalCargo = await cargoFile.text();
const packageStart = originalCargo.indexOf("[package]");
if (packageStart < 0) {
	throw new Error("Cargo.toml is missing the root [package] section");
}

const nextSectionOffset = originalCargo.slice(packageStart + "[package]".length).search(/^\[/m);
const packageEnd = nextSectionOffset < 0 ? originalCargo.length : packageStart + "[package]".length + nextSectionOffset;
const packageSection = originalCargo.slice(packageStart, packageEnd);
const versionAssignments = [...packageSection.matchAll(/^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
if (versionAssignments.length !== 1) {
	throw new Error(`Expected exactly one root [package].version assignment, found ${versionAssignments.length}`);
}

const current = versionAssignments[0][1];
const currentMatch = current.match(semverRe);
if (!currentMatch) {
	throw new Error(`Could not parse current version: ${current}`);
}

const [, majorText, minorText, patchText] = currentMatch;
const major = Number.parseInt(majorText, 10);
const minor = Number.parseInt(minorText, 10);
const patch = Number.parseInt(patchText, 10);
let nextVersion: string;

if (bump === "patch") {
	nextVersion = `${major}.${minor}.${patch + 1}`;
} else if (bump === "minor") {
	nextVersion = `${major}.${minor + 1}.0`;
} else if (bump === "major") {
	nextVersion = `${major + 1}.0.0`;
} else if (semverRe.test(bump)) {
	nextVersion = bump;
} else {
	throw new Error(`Invalid argument: ${bump}. Expected patch, minor, major, or x.y.z`);
}

const versionLine = /^(\s*version\s*=\s*")[^"]+("\s*(?:#.*)?$)/m;
const updatedPackageSection = packageSection.replace(versionLine, `$1${nextVersion}$2`);
if (updatedPackageSection === packageSection) {
	throw new Error("Failed to update the root [package].version assignment");
}
let updatedCargo = originalCargo.slice(0, packageStart) + updatedPackageSection + originalCargo.slice(packageEnd);

// Workspace members inherit their crate version from [workspace.package]. Keep
// that Cargo-owned metadata in sync without treating it as another product
// version source; package.json and plugin metadata are intentionally untouched.
const workspaceStart = updatedCargo.indexOf("[workspace.package]");
if (workspaceStart >= 0) {
	const workspaceContentStart = workspaceStart + "[workspace.package]".length;
	const nextWorkspaceSectionOffset = updatedCargo.slice(workspaceContentStart).search(/^\[/m);
	const workspaceEnd = nextWorkspaceSectionOffset < 0 ? updatedCargo.length : workspaceContentStart + nextWorkspaceSectionOffset;
	const workspaceSection = updatedCargo.slice(workspaceStart, workspaceEnd);
	const workspaceVersions = [...workspaceSection.matchAll(/^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
	if (workspaceVersions.length !== 1 || workspaceVersions[0][1] !== current) {
		throw new Error("[workspace.package].version must contain exactly the current root package version");
	}
	const updatedWorkspaceSection = workspaceSection.replace(versionLine, `$1${nextVersion}$2`);
	updatedCargo = updatedCargo.slice(0, workspaceStart) + updatedWorkspaceSection + updatedCargo.slice(workspaceEnd);
}

console.log(`Bumping ${current} → ${nextVersion}`);
if (mode === "dry-run") {
	console.log("Dry run: no files, commits, tags, or pushes changed.");
	process.exit(0);
}

function run(command: string[], capture = false): string {
	const result = Bun.spawnSync(command, {
		stdout: capture ? "pipe" : "inherit",
		stderr: "inherit",
	});
	if (!result.success) {
		throw new Error(`Command failed (${result.exitCode}): ${command.join(" ")}`);
	}
	return capture ? new TextDecoder().decode(result.stdout) : "";
}

if (mode === "release" && run(["git", "status", "--porcelain"], true).trim() !== "") {
	throw new Error("Working tree must be clean before creating a release");
}

const lockPath = "Cargo.lock";
const lockFile = Bun.file(lockPath);
const originalLock = (await lockFile.exists()) ? await lockFile.text() : undefined;
await Bun.write(cargoPath, updatedCargo);

try {
	console.log("Running Rust release checks...");
	run(["cargo", "fmt", "--all", "--check"]);
	run(["cargo", "check", "--workspace", "--all-targets"]);
	run(["cargo", "test", "--workspace", "--all-targets"]);
	run(["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
} catch (error) {
	await Bun.write(cargoPath, originalCargo);
	if (originalLock === undefined) {
		if (await lockFile.exists()) await Bun.write(lockPath, "");
	} else {
		await Bun.write(lockPath, originalLock);
	}
	throw error;
}

if (mode === "prepare") {
	console.log(`Prepared v${nextVersion}; no commit, tag, or push was performed.`);
	process.exit(0);
}

run(["git", "add", cargoPath, lockPath]);
run(["git", "commit", "-m", `Release v${nextVersion}`]);
run(["git", "tag", `v${nextVersion}`]);
run(["git", "push"]);
run(["git", "push", "origin", `v${nextVersion}`]);
console.log(`Released v${nextVersion}`);
