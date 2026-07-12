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
const nameAssignments = [...packageSection.matchAll(/^\s*name\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
if (nameAssignments.length !== 1) {
	throw new Error(`Expected exactly one root [package].name assignment, found ${nameAssignments.length}`);
}
const rootPackageName = nameAssignments[0][1];
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

const workspacePackageNames = [rootPackageName];
const workspaceStartForMembers = originalCargo.indexOf("[workspace]");
if (workspaceStartForMembers >= 0) {
	const workspaceContentStart = workspaceStartForMembers + "[workspace]".length;
	const nextWorkspaceSectionOffset = originalCargo.slice(workspaceContentStart).search(/^\[/m);
	const workspaceEnd = nextWorkspaceSectionOffset < 0 ? originalCargo.length : workspaceContentStart + nextWorkspaceSectionOffset;
	const workspaceSection = originalCargo.slice(workspaceStartForMembers, workspaceEnd);
	const membersMatch = workspaceSection.match(/members\s*=\s*\[([\s\S]*?)\]/m);
	if (membersMatch) {
		for (const memberPath of membersMatch[1].matchAll(/"([^"]+)"/g)) {
			const memberCargo = await Bun.file(`${memberPath[1]}/Cargo.toml`).text();
			const memberPackageStart = memberCargo.indexOf("[package]");
			const memberPackageEndOffset = memberPackageStart < 0 ? -1 : memberCargo.slice(memberPackageStart + "[package]".length).search(/^\[/m);
			const memberPackageEnd = memberPackageEndOffset < 0 ? memberCargo.length : memberPackageStart + "[package]".length + memberPackageEndOffset;
			const memberPackageSection = memberCargo.slice(memberPackageStart, memberPackageEnd);
			const memberName = memberPackageSection.match(/^\s*name\s*=\s*"([^"]+)"\s*(?:#.*)?$/m)?.[1];
			if (!memberName) {
				throw new Error(`Workspace member ${memberPath[1]} is missing a root package name`);
			}
			workspacePackageNames.push(memberName);
		}
	}
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
const originalLock = (await Bun.file(lockPath).exists()) ? await Bun.file(lockPath).text() : undefined;

function lockPackages(lock: string): Map<string, string> {
	const packages = new Map<string, string>();
	for (const body of lock.split(/^\[\[package\]\]\n/m).slice(1)) {
		const section = `[[package]]\n${body}`;
		const name = section.match(/^name = "([^"]+)"$/m)?.[1];
		if (!name) throw new Error("Cargo.lock contains a package without a name");
		packages.set(name, section);
	}
	return packages;
}

function verifyLockRefresh(original: string | undefined, updated: string, expectedVersion: string): void {
	const originalPackages = original === undefined ? new Map<string, string>() : lockPackages(original);
	const updatedPackages = lockPackages(updated);
	if (original !== undefined && original.split(/\n(?=\[\[package\]\])/m)[0] !== updated.split(/\n(?=\[\[package\]\])/m)[0]) {
		throw new Error("Cargo.lock header changed during the narrow version refresh");
	}
	for (const packageName of workspacePackageNames) {
		const before = originalPackages.get(packageName);
		const after = updatedPackages.get(packageName);
		if (!after) throw new Error(`Cargo.lock is missing workspace package ${packageName}`);
		if (before && !new RegExp(`^version = "${current.replaceAll(".", "\\.")}"$`, "m").test(before)) {
			throw new Error(`Cargo.lock workspace package ${packageName} did not start at ${current}`);
		}
		if (!new RegExp(`^version = "${expectedVersion.replaceAll(".", "\\.")}"$`, "m").test(after)) {
			throw new Error(`Cargo.lock workspace package ${packageName} was not refreshed to ${expectedVersion}`);
		}
	}
	for (const [packageName, before] of originalPackages) {
		if (!updatedPackages.has(packageName)) {
			throw new Error(`Cargo.lock package ${packageName} disappeared during the narrow version refresh`);
		}
		if (!workspacePackageNames.includes(packageName) && updatedPackages.get(packageName) !== before) {
			throw new Error(`Cargo.lock dependency ${packageName} changed outside the workspace version refresh`);
		}
	}
	for (const packageName of updatedPackages.keys()) {
		if (!originalPackages.has(packageName) && !workspacePackageNames.includes(packageName)) {
			throw new Error(`Cargo.lock gained dependency ${packageName} during the narrow version refresh`);
		}
	}
}

await Bun.write(cargoPath, updatedCargo);

try {
	console.log("Refreshing only workspace package versions in Cargo.lock...");
	run(["cargo", "update", "-p", rootPackageName, "--precise", nextVersion]);
	const refreshedLock = await Bun.file(lockPath).text();
	verifyLockRefresh(originalLock, refreshedLock, nextVersion);

	console.log("Running locked Rust release checks...");
	run(["cargo", "fmt", "--all", "--check"]);
	run(["cargo", "check", "--workspace", "--all-targets", "--locked"]);
	run(["cargo", "test", "--workspace", "--all-targets", "--locked"]);
	run(["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"]);
} catch (error) {
	await Bun.write(cargoPath, originalCargo);
	if (originalLock === undefined) {
		if (await Bun.file(lockPath).exists()) await Bun.write(lockPath, "");
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
